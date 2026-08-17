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
use futures::stream::{self, Stream};
use std::{convert::Infallible, sync::Arc, time::Duration};

const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Handle incoming MCP Streamable HTTP requests at `/mcp`.
/// Supports POST JSON-RPC, GET SSE streams, and DELETE session teardown.
pub async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
) -> Response {
    match *req.method() {
        Method::GET => handle_mcp_get(state, req.headers()).await,
        Method::DELETE => handle_mcp_delete(state, req.headers()).await,
        _ => handle_mcp_post(state, req).await,
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

    let Some(session_id) = session_id else {
        return sse_keep_alive(stream::pending()).into_response();
    };

    if !state.mcp_sessions.contains(session_id).await {
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

    sse_keep_alive(stream::pending()).into_response()
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
            jsonrpc::make_error(jsonrpc::Id::Number(0), jsonrpc::PARSE_ERROR, "Invalid JSON-RPC")
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
        Some(state.mcp_sessions.create().await)
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
    use axum::{body::Body, http::Request, Router};
    use std::time::SystemTime;
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
    async fn get_without_session_returns_sse_stream() {
        let data_dir = temp_data_dir("get-sse");
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

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
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
}
