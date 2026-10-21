use crate::sidecar::http::AppState;
use crate::sidecar::mcp::jsonrpc;
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Method, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use futures::stream::Stream;
use std::{convert::Infallible, sync::Arc, time::Duration};

const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Total GET SSE stream lifetime; the server closes the stream afterwards and
/// clients are expected to reconnect (MCP Streamable HTTP spec).
const SSE_STREAM_MAX_LIFETIME: Duration = Duration::from_secs(30 * 60);
/// 目录变更通知的去抖窗口。
const TOOLS_CHANGED_DEBOUNCE: Duration = Duration::from_millis(500);

/// Handle incoming MCP Streamable HTTP requests at `/mcp`.
/// Supports POST JSON-RPC, GET SSE streams, and DELETE session teardown.
pub async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
) -> Response {
    match *req.method() {
        Method::GET => handle_mcp_get(state, req.headers()).await,
        Method::DELETE => handle_mcp_delete(state, req.headers()).await,
        Method::POST => handle_mcp_post(state, req).await,
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn handle_mcp_get(state: Arc<AppState>, headers: &HeaderMap) -> Response {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !accept.contains("text/event-stream") {
        return (
            StatusCode::NOT_ACCEPTABLE,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": jsonrpc::INVALID_REQUEST,
                    "message": "GET requires Accept: text/event-stream"
                }
            })
            .to_string(),
        )
            .into_response();
    }

    let session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());

    // Anonymous streams have no legitimate use: spec clients always send the
    // session id obtained from initialize. Allowing them lets LAN clients open
    // unlimited keep-alive streams.
    let Some(session_id) = session_id else {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": jsonrpc::INVALID_REQUEST,
                    "message": "Missing Mcp-Session-Id header"
                }
            })
            .to_string(),
        )
            .into_response();
    };

    if !state
        .mcp_sessions
        .validate_and_touch(session_id, session_idle_ttl(&state).await)
        .await
    {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": jsonrpc::INVALID_REQUEST,
                    "message": "Unknown MCP session"
                }
            })
            .to_string(),
        )
            .into_response();
    }

    // 事件先进入会话自己的通道，每个流各自合并：窗口内继续到来的变更会推后窗口结束，
    // 因此客户端收到通知时看到的一定是这一批变更的最终状态。
    state
        .mcp_sessions
        .ensure_event_forwarder(state.event_bus.clone())
        .await;
    let Some(receiver) = state.mcp_sessions.subscribe(session_id).await else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": jsonrpc::INVALID_REQUEST, "message": "Unknown MCP session" }
            })
            .to_string(),
        )
            .into_response();
    };

    let bounded = futures::StreamExt::take_until(
        merged_tool_notifications(receiver),
        tokio::time::sleep(SSE_STREAM_MAX_LIFETIME),
    );
    sse_keep_alive(bounded).into_response()
}

struct MergeState {
    receiver: tokio::sync::broadcast::Receiver<()>,
    pending: bool,
    deadline: tokio::time::Instant,
}

/// 把窗口内的一串信号合并成一条 `notifications/tools/list_changed`。
fn merged_tool_notifications(
    receiver: tokio::sync::broadcast::Receiver<()>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    futures::stream::unfold(
        MergeState {
            receiver,
            pending: false,
            deadline: tokio::time::Instant::now(),
        },
        |mut state| async move {
            loop {
                if !state.pending {
                    if state.receiver.recv().await.is_err() {
                        return None;
                    }
                    state.pending = true;
                    state.deadline = tokio::time::Instant::now() + TOOLS_CHANGED_DEBOUNCE;
                    continue;
                }
                tokio::select! {
                    _ = tokio::time::sleep_until(state.deadline) => {
                        state.pending = false;
                        return Some((Ok::<_, Infallible>(tools_changed_event()), state));
                    }
                    received = state.receiver.recv() => {
                        if received.is_err() {
                            state.pending = false;
                            return Some((Ok(tools_changed_event()), state));
                        }
                        // 窗口内又有变更：推后结束，保证只播报最终状态。
                        state.deadline = tokio::time::Instant::now() + TOOLS_CHANGED_DEBOUNCE;
                    }
                }
            }
        },
    )
}

fn tools_changed_event() -> Event {
    Event::default().event("message").data(
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/list_changed",
        })
        .to_string(),
    )
}

async fn handle_mcp_delete(state: Arc<AppState>, headers: &HeaderMap) -> Response {
    let Some(session_id) = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    if state.mcp_sessions.remove(session_id).await {
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn handle_mcp_post(state: Arc<AppState>, req: axum::extract::Request) -> Response {
    let headers = req.headers().clone();
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let agent_info = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body_bytes = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Request body too large").into_response(),
    };

    if body_bytes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            jsonrpc::make_error(
                jsonrpc::Id::Number(0),
                jsonrpc::PARSE_ERROR,
                "Invalid JSON-RPC",
            )
            .to_string(),
        )
            .into_response();
    }

    let accepts_sse = accept.contains("text/event-stream");

    let raw_message: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                jsonrpc::make_error(
                    jsonrpc::Id::Number(0),
                    jsonrpc::PARSE_ERROR,
                    "Invalid JSON-RPC",
                )
                .to_string(),
            )
                .into_response()
        }
    };

    let method_name = raw_message.get("method").and_then(|m| m.as_str());
    if method_name != Some("initialize") {
        if let Some(session_id) = headers
            .get(MCP_SESSION_ID_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            if !state
                .mcp_sessions
                .validate_and_touch(session_id, session_idle_ttl(&state).await)
                .await
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    }

    if raw_message.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }

    let parsed = match jsonrpc::parse_request_value(&raw_message) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                jsonrpc::make_error(
                    jsonrpc::Id::Number(0),
                    jsonrpc::INVALID_REQUEST,
                    "Invalid JSON-RPC request",
                )
                .to_string(),
            )
                .into_response()
        }
    };

    let (id, method, params) = parsed;
    let new_session_id = if method == "initialize" {
        match state.mcp_sessions.create().await {
            Some(id) => Some(id),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::CONTENT_TYPE, "application/json")],
                    jsonrpc::make_error(
                        id,
                        jsonrpc::INTERNAL_ERROR,
                        "MCP session capacity reached",
                    )
                    .to_string(),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let response = crate::sidecar::mcp::server::handle_request(
        id,
        &method,
        params,
        state.clone(),
        agent_info.as_deref(),
    )
    .await;

    build_post_response(response, accepts_sse, new_session_id.as_deref())
}

fn build_post_response(
    response: serde_json::Value,
    accepts_sse: bool,
    session_id: Option<&str>,
) -> Response {
    let mut builder = Response::builder();

    if let Some(session_id) = session_id {
        if let Ok(value) = header::HeaderValue::from_str(session_id) {
            builder = builder.header(MCP_SESSION_ID_HEADER, value);
        }
    }

    if accepts_sse {
        let sse_data = format!("event: message\ndata: {}\n\n", response);
        builder
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(sse_data))
            .unwrap()
    } else {
        builder
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response.to_string()))
            .unwrap()
    }
}

/// Live session idle TTL from settings; falls back to the default when the
/// settings store is unreadable, so expiry checks never panic.
async fn session_idle_ttl(state: &AppState) -> Duration {
    crate::sidecar::services::settings::get_settings(state.db.as_ref())
        .map(|settings| Duration::from_millis(settings.advanced.mcp_session_idle_ttl_ms as u64))
        .unwrap_or(Duration::from_millis(
            crate::sidecar::services::settings::MCP_SESSION_IDLE_TTL_MS_DEFAULT as u64,
        ))
}

/// Periodically drops idle sessions so crashed clients don't leak entries.
pub fn spawn_session_sweeper(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let ttl = session_idle_ttl(&state).await;
            state.mcp_sessions.sweep_expired(ttl).await;
        }
    });
}

fn sse_keep_alive<S>(stream: S) -> impl IntoResponse
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::services::event_bus::Evt;
    use axum::{body::Body, http::Request, Router};
    use std::time::SystemTime;
    use tokio_stream::StreamExt as _;
    use tower::ServiceExt;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-mcp-http-{test_name}-{timestamp}"))
    }

    fn test_state(data_dir: std::path::PathBuf) -> Arc<AppState> {
        AppState::for_test(&data_dir)
    }

    #[tokio::test]
    async fn put_returns_method_not_allowed() {
        let data_dir = temp_data_dir("get-sse");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(test_state(data_dir.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/mcp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn get_without_session_returns_bad_request() {
        let data_dir = temp_data_dir("get-no-session");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(test_state(data_dir.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        assert!(String::from_utf8_lossy(&body).contains("Missing Mcp-Session-Id"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn post_with_unknown_session_returns_not_found() {
        let data_dir = temp_data_dir("post-unknown-session");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(test_state(data_dir.clone()));

        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .header(MCP_SESSION_ID_HEADER, "forged-session-id")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn initialize_returns_session_and_get_subscribes() {
        let data_dir = temp_data_dir("session-flow");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(test_state(data_dir.clone()));

        let init_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .body(Body::from(init_body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("initialize should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let session_id = response
            .headers()
            .get(MCP_SESSION_ID_HEADER)
            .expect("initialize should return session id")
            .to_str()
            .expect("session id should be valid header")
            .to_string();

        let get_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .header(MCP_SESSION_ID_HEADER, session_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("get should succeed");

        assert_eq!(get_response.status(), StatusCode::OK);
        assert_eq!(
            get_response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        let _ = std::fs::remove_dir_all(data_dir);
    }

    fn initialize_request_body() -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }
        })
        .to_string()
    }

    #[tokio::test(start_paused = true)]
    async fn post_after_idle_ttl_returns_not_found() {
        let data_dir = temp_data_dir("ttl-expiry");
        let state = test_state(data_dir.clone());
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(state.clone());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .body(Body::from(initialize_request_body()))
                    .unwrap(),
            )
            .await
            .expect("initialize should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        let session_id = response
            .headers()
            .get(MCP_SESSION_ID_HEADER)
            .expect("initialize should return session id")
            .to_str()
            .expect("session id should be valid header")
            .to_string();

        let ttl = crate::sidecar::services::settings::get_settings(state.db.as_ref())
            .expect("settings should be readable")
            .advanced
            .mcp_session_idle_ttl_ms;
        tokio::time::advance(Duration::from_millis(ttl as u64) + Duration::from_secs(1)).await;

        let expired = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id)
                    .body(Body::from(
                        serde_json::json!({
                            "jsonrpc": "2.0", "id": 2, "method": "tools/list"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("request should succeed");
        assert_eq!(expired.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test(start_paused = true)]
    async fn get_stream_ends_after_max_lifetime() {
        let data_dir = temp_data_dir("sse-lifetime");
        let state = test_state(data_dir.clone());
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(state.clone());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .body(Body::from(initialize_request_body()))
                    .unwrap(),
            )
            .await
            .expect("initialize should succeed");
        let session_id = response
            .headers()
            .get(MCP_SESSION_ID_HEADER)
            .expect("initialize should return session id")
            .to_str()
            .expect("session id should be valid header")
            .to_string();

        let get_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .header(MCP_SESSION_ID_HEADER, session_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("get should succeed");
        assert_eq!(get_response.status(), StatusCode::OK);

        // Paused time auto-advances while the body task waits on timers, so
        // consuming the body drives the stream to its lifetime bound.
        let bytes = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .expect("stream should end gracefully at max lifetime");
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("keep-alive"));
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn initialize_returns_503_at_session_capacity() {
        let data_dir = temp_data_dir("session-capacity");
        let state = test_state(data_dir.clone());
        while state.mcp_sessions.create().await.is_some() {}

        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json")
                    .body(Body::from(initialize_request_body()))
                    .unwrap(),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn get_stream_delivers_tools_list_changed() {
        let data_dir = temp_data_dir("tools-list-changed");
        let state = test_state(data_dir.clone());
        let session_id = state.mcp_sessions.create().await.expect("session created");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .header(MCP_SESSION_ID_HEADER, &session_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let mut stream = response.into_body().into_data_stream();
        state.event_bus.emit(Evt::ToolsChanged {
            reason: "test".into(),
        });

        let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("a notification should arrive within the timeout")
            .expect("stream should yield a frame")
            .expect("frame should be readable");
        let text = String::from_utf8_lossy(&frame);
        assert!(
            text.contains("notifications/tools/list_changed"),
            "unexpected frame: {text}"
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn stopping_a_server_also_announces_the_tool_list_change() {
        let data_dir = temp_data_dir("tools-list-changed-stop");
        let state = test_state(data_dir.clone());
        let session_id = state.mcp_sessions.create().await.expect("session created");
        let app = Router::new()
            .route("/mcp", axum::routing::any(handle_mcp_request))
            .with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/mcp")
                    .header(header::ACCEPT, "text/event-stream")
                    .header(MCP_SESSION_ID_HEADER, &session_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let mut stream = response.into_body().into_data_stream();
        state.event_bus.emit(Evt::ServerStatus {
            server_id: "server-1".into(),
            status: "stopped".into(),
            error_message: None,
        });

        let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("a stopped server must announce the change")
            .expect("stream should yield a frame")
            .expect("frame should be readable");
        assert!(
            String::from_utf8_lossy(&frame).contains("notifications/tools/list_changed"),
            "a stop hides tools, so the list changed"
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn a_burst_is_announced_once_after_the_window_closes() {
        let data_dir = temp_data_dir("tools-changed-burst");
        let state = test_state(data_dir.clone());
        let session_id = state.mcp_sessions.create().await.expect("session created");
        state
            .mcp_sessions
            .ensure_event_forwarder(state.event_bus.clone())
            .await;
        let receiver = state
            .mcp_sessions
            .subscribe(&session_id)
            .await
            .expect("session has a channel");
        let mut notifications = Box::pin(merged_tool_notifications(receiver));

        for _ in 0..5 {
            state.event_bus.emit(Evt::ToolsChanged {
                reason: "burst".into(),
            });
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let started = std::time::Instant::now();
        let first = tokio::time::timeout(Duration::from_secs(2), notifications.next())
            .await
            .expect("a merged notification must arrive");
        assert!(first.is_some(), "the burst must produce one notification");
        assert!(
            started.elapsed() >= Duration::from_millis(400),
            "the window must close before announcing, so readers see the final state"
        );

        let second = tokio::time::timeout(Duration::from_millis(300), notifications.next()).await;
        assert!(
            second.is_err(),
            "a single burst must not produce a second notification"
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
