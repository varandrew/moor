pub mod app_error;
mod auth;
pub mod routes;

#[cfg(test)]
mod oauth_regression_tests;

use crate::sidecar::db::Database;
use crate::sidecar::mcp::transport::mcp_session::McpSessionStore;
use crate::sidecar::services::event_bus::EventBus;
use crate::sidecar::services::server_manager::ServerManager;
use axum::{http::StatusCode, middleware, response::IntoResponse, Json, Router};
use serde_json::json;
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct AppState {
    pub db: Arc<Database>,
    pub api_token: String,
    pub version: String,
    pub port: u16,
    pub mcp_sessions: Arc<McpSessionStore>,
    pub event_bus: Arc<EventBus>,
    pub server_manager: Arc<ServerManager>,
}

impl AppState {
    /// 生产构造函数:组装 axum 共享状态的全部协作者。
    pub fn new(
        db: Arc<Database>,
        api_token: String,
        version: String,
        port: u16,
        event_bus: Arc<EventBus>,
        server_manager: Arc<ServerManager>,
    ) -> Self {
        Self {
            db,
            api_token,
            version,
            port,
            mcp_sessions: Arc::new(McpSessionStore::new()),
            event_bus,
            server_manager,
        }
    }

    /// 测试构造函数:在一个临时目录里 open+migrate 一个全新 Database,
    /// 初始化默认 settings,装上默认 EventBus + 真实 ServerManager,
    /// 返回可直接喂给路由的 AppState。
    #[cfg(test)]
    pub fn for_test(data_dir: &std::path::Path) -> Arc<Self> {
        Self::for_test_with_credential_store(
            data_dir,
            Arc::new(
                crate::sidecar::services::server_authorization::MemoryCredentialStore::default(),
            ),
        )
    }

    #[cfg(test)]
    pub fn for_test_with_credential_store(
        data_dir: &std::path::Path,
        store: Arc<dyn crate::sidecar::services::server_authorization::CredentialStore>,
    ) -> Arc<Self> {
        use crate::sidecar::db::Database;
        use crate::sidecar::services::settings;
        std::fs::create_dir_all(data_dir).expect("failed to create temp data dir");
        let db = Arc::new(Database::open(&data_dir.join("moor.db")).expect("failed to open db"));
        db.run_migrations().expect("failed to run migrations");
        settings::init_settings(db.as_ref(), data_dir).expect("failed to init settings");
        let event_bus = Arc::new(EventBus::new(16));
        Arc::new(Self::new(
            db.clone(),
            "test-token".to_string(),
            "test".to_string(),
            19323,
            event_bus.clone(),
            Arc::new(ServerManager::new_with_credential_store(
                db, event_bus, store,
            )),
        ))
    }
}

pub fn create_app(state: Arc<AppState>) -> Router {
    let mcp_routes = Router::new().route(
        "/mcp",
        axum::routing::any(
            crate::sidecar::mcp::transport::streamable_http_server::handle_mcp_request,
        ),
    );

    Router::new()
        .merge(routes::health::router())
        .merge(routes::servers::router())
        .merge(routes::server_authorization::router())
        .merge(routes::server_health::router())
        .merge(routes::updates::router())
        .merge(routes::profiles::router())
        .merge(routes::logs::router())
        .merge(routes::settings::router())
        .merge(routes::events::router())
        .merge(routes::import_routes::router())
        .merge(mcp_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::sidecar::http::auth::auth_middleware,
        ))
        .with_state(state)
}

pub fn json_error_response(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> axum::response::Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
        .into_response()
}

pub async fn start_server(state: Arc<AppState>, host: &str, port: u16) -> Result<(), String> {
    crate::sidecar::mcp::transport::streamable_http_server::spawn_session_sweeper(state.clone());
    crate::sidecar::services::audit_recorder::spawn_retention_sweeper(state.db.clone());
    let addr = format!("{host}:{port}");
    let std_listener =
        crate::bind_listener(host, port).map_err(|e| format!("Failed to bind {addr}: {e}"))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to bind {addr}: {e}"))?;
    let listener =
        TcpListener::from_std(std_listener).map_err(|e| format!("Failed to bind {addr}: {e}"))?;
    let app = create_app(state);
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, routing::post};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::sync::mpsc;

    async fn spawn_router(app: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, server)
    }

    #[tokio::test]
    async fn gateway_preserves_static_credentials_across_upstream_request_types() {
        let (methods, mut received) = mpsc::unbounded_channel();
        let upstream = Router::new().route(
            "/mcp",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let methods = methods.clone();
                async move {
                    let authorization = headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    let method = body["method"].as_str().unwrap_or_default().to_string();
                    methods.send((method.clone(), authorization)).unwrap();
                    if method == "notifications/initialized" {
                        return StatusCode::ACCEPTED.into_response();
                    }
                    let result = match method.as_str() {
                        "initialize" => json!({
                            "protocolVersion": "2025-03-26",
                            "capabilities": { "tools": {} },
                            "serverInfo": { "name": "protected", "version": "1.0.0" }
                        }),
                        "tools/list" => json!({
                            "tools": [{
                                "name": "echo",
                                "description": "Echo input",
                                "inputSchema": { "type": "object" }
                            }]
                        }),
                        "tools/call" => json!({
                            "content": [{ "type": "text", "text": "ok" }]
                        }),
                        _ => json!({}),
                    };
                    Json(json!({ "jsonrpc": "2.0", "id": body["id"], "result": result }))
                        .into_response()
                }
            }),
        );
        let (upstream_addr, upstream_server) = spawn_router(upstream).await;
        let data_dir =
            std::env::temp_dir().join(format!("moor-http-auth-{}", uuid::Uuid::new_v4()));
        let state = AppState::for_test(&data_dir);
        crate::sidecar::db::profile_repo::ProfileRepository::new(&state.db)
            .seed_default()
            .unwrap();
        let (gateway_addr, gateway_server) = spawn_router(create_app(state)).await;
        let gateway = format!("http://{gateway_addr}");
        let client = reqwest::Client::new();

        let create = client
            .post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({
                "name": "protected",
                "connectionType": "http",
                "url": format!("http://{upstream_addr}/mcp"),
                "headers": { "Authorization": "Bearer static-token" }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let server_id = create.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let profiles = client
            .get(format!("{gateway}/api/profiles"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let profile_id = profiles
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["isActive"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap();
        let direct = client
            .put(format!("{gateway}/api/profiles/{profile_id}"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "exposureMode": "direct" }))
            .send()
            .await
            .unwrap();
        assert_eq!(direct.status(), StatusCode::OK);
        for path in ["start", "tools/refresh", "check-health"] {
            let response = client
                .post(format!("{gateway}/api/servers/{server_id}/{path}"))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
        }

        let initialize = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "test-agent", "version": "1.0.0" }
                }
            }))
            .send()
            .await
            .unwrap();
        let session_id = initialize
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let call = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": { "value": "ok" } }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(call.status(), StatusCode::OK);
        let call_body = call.json::<Value>().await.unwrap();
        assert_eq!(
            call_body["result"]["content"][0]["text"], "ok",
            "{call_body}"
        );

        let mut requests = Vec::new();
        while let Ok(request) = received.try_recv() {
            requests.push(request);
        }
        assert!(requests
            .iter()
            .all(|(_, authorization)| authorization == "Bearer static-token"));
        for method in [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "ping",
            "tools/call",
        ] {
            assert!(
                requests.iter().any(|(actual, _)| actual == method),
                "missing {method}: {requests:?}"
            );
        }
        assert!(
            requests
                .iter()
                .filter(|(method, _)| method == "tools/list")
                .count()
                >= 2
        );

        gateway_server.abort();
        upstream_server.abort();
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn preregistered_oauth_completes_callback_and_authorizes_upstream_requests() {
        let (token_requests, mut token_forms) = mpsc::unbounded_channel();
        let (registrations, mut registration_forms) = mpsc::unbounded_channel();
        let refresh_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let token_refresh_count = refresh_count.clone();
        let reject_oauth = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reject_retried = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let insufficient_scope = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let missing_scope = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (revocations, mut revocation_forms) = mpsc::unbounded_channel();
        let revoke_offline = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let revocation_offline = revoke_offline.clone();
        let revoke_hang = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let revocation_hang = revoke_hang.clone();
        let revoke_waiting = Arc::new(tokio::sync::Notify::new());
        let revocation_waiting = revoke_waiting.clone();
        let token_waiting = Arc::new(tokio::sync::Notify::new());
        let token_continue = Arc::new(tokio::sync::Notify::new());
        let hold_refresh = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hold_discovery = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let discovery_waiting = Arc::new(tokio::sync::Notify::new());
        let discovery_continue = Arc::new(tokio::sync::Notify::new());
        let upstream_hold_discovery = hold_discovery.clone();
        let upstream_discovery_waiting = discovery_waiting.clone();
        let upstream_discovery_continue = discovery_continue.clone();
        let handler_waiting = token_waiting.clone();
        let handler_continue = token_continue.clone();
        let handler_hold_refresh = hold_refresh.clone();
        let authorization_server = Router::new()
            .route(
                "/.well-known/oauth-authorization-server",
                axum::routing::get(|headers: HeaderMap| async move {
                    let base = format!("http://{}", headers.get("host").unwrap().to_str().unwrap());
                    Json(json!({
                        "issuer": base,
                        "authorization_endpoint": format!("{base}/authorize"),
                        "token_endpoint": format!("{base}/token"),
                        "registration_endpoint": format!("{base}/register"),
                        "revocation_endpoint": format!("{base}/revoke"),
                        "code_challenge_methods_supported": ["S256"],
                        "authorization_response_iss_parameter_supported": true
                    }))
                }),
            )
            .route(
                "/register",
                post(move |Json(body): Json<Value>| {
                    let registrations = registrations.clone();
                    async move {
                        registrations.send(body).unwrap();
                        Json(json!({
                            "client_id": "dynamic-client-id",
                            "client_secret": "dynamic-client-secret"
                        }))
                    }
                }),
            )
            .route(
                "/revoke",
                post(
                    move |axum::Form(form): axum::Form<HashMap<String, String>>| {
                        let revocations = revocations.clone();
                        let offline = revocation_offline.clone();
                        let hang = revocation_hang.clone();
                        let waiting = revocation_waiting.clone();
                        async move {
                            revocations.send(form).unwrap();
                            if hang.load(std::sync::atomic::Ordering::SeqCst) {
                                waiting.notify_one();
                                std::future::pending::<()>().await;
                            }
                            if offline.load(std::sync::atomic::Ordering::SeqCst) {
                                StatusCode::SERVICE_UNAVAILABLE.into_response()
                            } else {
                                StatusCode::OK.into_response()
                            }
                        }
                    },
                ),
            )
            .route(
                "/token",
                post(
                    move |axum::Form(form): axum::Form<HashMap<String, String>>| {
                        let token_requests = token_requests.clone();
                        let waiting = handler_waiting.clone();
                        let resume = handler_continue.clone();
                        let hold_refresh = handler_hold_refresh.clone();
                        async move {
                            token_requests.send(form.clone()).unwrap();
                            if form.get("code").map(String::as_str) == Some("hold-code")
                                || (form["grant_type"] == "refresh_token"
                                    && hold_refresh.load(std::sync::atomic::Ordering::SeqCst))
                            {
                                waiting.notify_one();
                                resume.notified().await;
                            }
                            if form.get("refresh_token").map(String::as_str)
                                == Some("rotated-refresh-token")
                            {
                                return StatusCode::BAD_REQUEST.into_response();
                            }
                            if form["grant_type"] == "refresh_token" {
                                let refresh = token_refresh_count
                                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let access_token = match refresh {
                                    0 => "refreshed-access-token",
                                    1 => "retried-access-token",
                                    _ => "rejected-access-token",
                                };
                                return Json(json!({
                                    "access_token": access_token,
                                    "refresh_token": "rotated-refresh-token",
                                    "token_type": "Bearer",
                                    "expires_in": 3600
                                }))
                                .into_response();
                            }
                            if form["code"] == "bad-code" {
                                return StatusCode::BAD_REQUEST.into_response();
                            }
                            Json(json!({
                                "access_token": "oauth-access-token",
                                "refresh_token": "oauth-refresh-token",
                                "token_type": "Bearer",
                                "expires_in": 3600
                            }))
                            .into_response()
                        }
                    },
                ),
            );
        let (as_addr, as_server) = spawn_router(authorization_server).await;
        let issuer = format!("http://{as_addr}");
        let (upstream_requests, mut upstream) = mpsc::unbounded_channel();
        let issuer_for_resource = issuer.clone();
        let reject_oauth_upstream = reject_oauth.clone();
        let reject_retried_upstream = reject_retried.clone();
        let insufficient_scope_upstream = insufficient_scope.clone();
        let missing_scope_upstream = missing_scope.clone();
        let protected = Router::new()
            .route(
                "/custom-resource-metadata",
                axum::routing::get(move || {
                    let issuer = issuer_for_resource.clone();
                    async move {
                        Json(json!({
                            "resource": "http://protected.example/mcp",
                            "authorization_servers": [issuer],
                            "scopes_supported": ["mcp.read"]
                        }))
                    }
                }),
            )
            .route(
                "/mcp",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let upstream_requests = upstream_requests.clone();
                    let hold_discovery = upstream_hold_discovery.clone();
                    let discovery_waiting = upstream_discovery_waiting.clone();
                    let discovery_continue = upstream_discovery_continue.clone();
                    async move {
                        let authorization = headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string();
                        if insufficient_scope_upstream.load(std::sync::atomic::Ordering::SeqCst)
                            && body["method"] == "tools/call"
                        {
                            let challenge = if missing_scope_upstream
                                .load(std::sync::atomic::Ordering::SeqCst)
                            {
                                "Bearer error=\"insufficient_scope\""
                            } else {
                                "Bearer error=\"insufficient_scope\", scope=\"mcp.write\""
                            };
                            return (
                                StatusCode::FORBIDDEN,
                                [("www-authenticate", challenge)],
                            )
                                .into_response();
                        }
                        if (reject_oauth_upstream.load(std::sync::atomic::Ordering::SeqCst)
                            && authorization == "Bearer oauth-access-token")
                            || (reject_retried_upstream.load(std::sync::atomic::Ordering::SeqCst)
                                && matches!(authorization.as_str(), "Bearer retried-access-token" | "Bearer rejected-access-token"))
                        {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        if authorization.is_empty() {
                            if hold_discovery.load(std::sync::atomic::Ordering::SeqCst) {
                                discovery_waiting.notify_one();
                                discovery_continue.notified().await;
                            }
                            let host = headers.get("host").unwrap().to_str().unwrap();
                            let challenge = format!(
                                "Bearer resource_metadata=\"http://{host}/custom-resource-metadata\", scope=\"mcp.read\""
                            );
                            return (StatusCode::UNAUTHORIZED, [("www-authenticate", challenge)])
                                .into_response();
                        }
                        let method = body["method"].as_str().unwrap_or_default().to_string();
                        upstream_requests.send((method.clone(), authorization)).unwrap();
                        if method == "notifications/initialized" {
                            return StatusCode::ACCEPTED.into_response();
                        }
                        let result = match method.as_str() {
                            "initialize" => json!({
                                "protocolVersion": "2025-03-26",
                                "capabilities": { "tools": {} },
                                "serverInfo": { "name": "protected", "version": "1.0.0" }
                            }),
                            "tools/list" => json!({
                                "tools": [{ "name": "echo", "inputSchema": { "type": "object" } }]
                            }),
                            "tools/call" => json!({
                                "content": [{ "type": "text", "text": "oauth-ok" }]
                            }),
                            _ => json!({}),
                        };
                        Json(json!({ "jsonrpc": "2.0", "id": body["id"], "result": result }))
                            .into_response()
                    }
                }),
            );
        let (upstream_addr, upstream_server) = spawn_router(protected).await;
        let data_dir = std::env::temp_dir().join(format!("moor-oauth-{}", uuid::Uuid::new_v4()));
        let credentials = Arc::new(
            crate::sidecar::services::server_authorization::MemoryCredentialStore::default(),
        );
        let state = AppState::for_test_with_credential_store(&data_dir, credentials.clone());
        crate::sidecar::db::profile_repo::ProfileRepository::new(&state.db)
            .seed_default()
            .unwrap();
        let mut events = state.event_bus.subscribe();
        let (gateway_addr, gateway_server) = spawn_router(create_app(state.clone())).await;
        let gateway = format!("http://{gateway_addr}");
        let client = reqwest::Client::new();
        let create = client
            .post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({
                "name": "protected",
                "connectionType": "http",
                "url": format!("http://{upstream_addr}/mcp")
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let server_id = create.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let initial = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(initial["state"], "not_required");

        let registered = client
            .put(format!(
                "{gateway}/api/servers/{server_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "moor-test-client", "clientSecret": "client-secret" }))
            .send()
            .await
            .unwrap();
        assert_eq!(registered.status(), StatusCode::OK);
        let started = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(started.status(), StatusCode::OK);
        let started = started.json::<Value>().await.unwrap();
        let authorization_url = started["authorizationUrl"].as_str().unwrap();
        let parsed = reqwest::Url::parse(authorization_url).unwrap();
        let query: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], "moor-test-client");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["resource"], format!("http://{upstream_addr}/mcp"));
        assert_eq!(query["scope"], "mcp.read");
        assert!(!query["code_challenge"].is_empty());
        let mut callback_url = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
        callback_url
            .query_pairs_mut()
            .append_pair("code", "test-code")
            .append_pair("state", &query["state"])
            .append_pair("iss", &issuer);
        let callback = client.get(callback_url).send().await.unwrap();
        let callback_status = callback.status();
        let callback_body = callback.text().await.unwrap();
        assert_eq!(callback_status, StatusCode::OK, "{callback_body}");

        let authorization = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(authorization["state"], "authorized");
        assert!(!authorization.to_string().contains("oauth-access-token"));
        assert!(!authorization.to_string().contains("client-secret"));
        let unchanged_client = client
            .put(format!(
                "{gateway}/api/servers/{server_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "moor-test-client", "clientSecret": "client-secret" }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(unchanged_client["state"], "authorized");
        let event = loop {
            let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(event.name(), "server:authorization");
            if event.payload()["state"] == "authorized" {
                break event;
            }
        };
        assert_eq!(event.payload()["state"], "authorized");

        let token_form = token_forms.recv().await.unwrap();
        assert_eq!(token_form["grant_type"], "authorization_code");
        assert_eq!(token_form["code"], "test-code");
        assert_eq!(token_form["client_id"], "moor-test-client");
        assert_eq!(
            token_form.get("client_secret").map(String::as_str),
            Some("client-secret"),
            "{token_form:?}"
        );
        assert_eq!(
            token_form["resource"],
            format!("http://{upstream_addr}/mcp")
        );
        assert!(!token_form["code_verifier"].is_empty());

        let profiles = client
            .get(format!("{gateway}/api/profiles"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let profile_id = profiles[0]["id"].as_str().unwrap();
        let direct = client
            .put(format!("{gateway}/api/profiles/{profile_id}"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "exposureMode": "direct" }))
            .send()
            .await
            .unwrap();
        assert_eq!(direct.status(), StatusCode::OK);
        let started_server = client
            .post(format!("{gateway}/api/servers/{server_id}/start"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(started_server.status(), StatusCode::OK);
        let initialize = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26", "capabilities": {},
                    "clientInfo": { "name": "test-agent", "version": "1.0.0" }
                }
            }))
            .send()
            .await
            .unwrap();
        let session_id = initialize
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap();
        let call = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            call.json::<Value>().await.unwrap()["result"]["content"][0]["text"],
            "oauth-ok"
        );
        let mut authorized_requests = 0;
        while let Ok((_, authorization)) = upstream.try_recv() {
            if !authorization.is_empty() {
                authorized_requests += 1;
                assert_eq!(authorization, "Bearer oauth-access-token");
            }
        }
        assert!(authorized_requests >= 4);
        state
            .db
            .run(
                "UPDATE server_authorizations SET expires_at = ?1 WHERE server_id = ?2",
                &[&"2000-01-01T00:00:00Z", &server_id],
            )
            .unwrap();
        let expired_call = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 20, "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            expired_call.json::<Value>().await.unwrap()["result"]["content"][0]["text"],
            "oauth-ok"
        );
        let refresh_forms: Vec<HashMap<String, String>> =
            std::iter::from_fn(|| token_forms.try_recv().ok()).collect();
        let refresh = refresh_forms
            .iter()
            .find(|form| form["grant_type"] == "refresh_token")
            .expect("expired token should be refreshed");
        assert_eq!(refresh["refresh_token"], "oauth-refresh-token");
        assert_eq!(refresh["client_id"], "moor-test-client");
        assert_eq!(refresh["client_secret"], "client-secret");
        assert_eq!(refresh["resource"], format!("http://{upstream_addr}/mcp"));
        let refreshed = std::iter::from_fn(|| upstream.try_recv().ok())
            .any(|(_, authorization)| authorization == "Bearer refreshed-access-token");
        assert!(refreshed);
        tokio::time::sleep(Duration::from_secs(2)).await;
        state
            .db
            .run(
                "UPDATE server_authorizations SET expires_at = ?1 WHERE server_id = ?2",
                &[&"2000-01-01T00:00:00Z", &server_id],
            )
            .unwrap();
        let rejected = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 21, "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert!(rejected.json::<Value>().await.unwrap()["error"].is_object());
        let rejected_snapshot = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(rejected_snapshot["state"], "required");
        assert_eq!(rejected_snapshot["errorCode"], "refresh_rejected");

        let stored = state.db.query_all(
            "SELECT client_secret_ref, access_token_ref, refresh_token_ref FROM server_authorizations",
            &[],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        );
        assert!(stored.is_ok());
        for (client_ref, access_ref, refresh_ref) in stored.unwrap() {
            assert_ne!(client_ref.as_deref(), Some("client-secret"));
            assert_ne!(access_ref.as_deref(), Some("oauth-access-token"));
            assert_ne!(refresh_ref.as_deref(), Some("oauth-refresh-token"));
        }

        let first_restart = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let first_url =
            reqwest::Url::parse(first_restart["authorizationUrl"].as_str().unwrap()).unwrap();
        let first_query: HashMap<String, String> = first_url.query_pairs().into_owned().collect();
        let abandoned_state = AppState::for_test(&data_dir);
        let (abandoned_addr, abandoned_gateway) = spawn_router(create_app(abandoned_state)).await;
        let abandoned = client
            .get(format!(
                "http://{abandoned_addr}/api/servers/{server_id}/authorization"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(abandoned["state"], "required");
        abandoned_gateway.abort();
        let second_restart = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let second_url =
            reqwest::Url::parse(second_restart["authorizationUrl"].as_str().unwrap()).unwrap();
        let second_query: HashMap<String, String> = second_url.query_pairs().into_owned().collect();
        let mut stale_callback = reqwest::Url::parse(&second_query["redirect_uri"]).unwrap();
        stale_callback
            .query_pairs_mut()
            .append_pair("code", "stale-code")
            .append_pair("state", &first_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(stale_callback).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let mut valid_callback = reqwest::Url::parse(&second_query["redirect_uri"]).unwrap();
        valid_callback
            .query_pairs_mut()
            .append_pair("code", "second-code")
            .append_pair("state", &second_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client
                .get(valid_callback.clone())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let replay = client.get(valid_callback).send().await;
        assert!(replay.is_err() || replay.unwrap().status() == StatusCode::BAD_REQUEST);

        reject_oauth.store(true, std::sync::atomic::Ordering::SeqCst);
        let retried = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 30, "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            retried.json::<Value>().await.unwrap()["result"]["content"][0]["text"],
            "oauth-ok"
        );
        assert!(std::iter::from_fn(|| upstream.try_recv().ok())
            .any(|(_, authorization)| authorization == "Bearer retried-access-token"));
        reject_retried.store(true, std::sync::atomic::Ordering::SeqCst);
        let bounded = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 31, "method": "tools/call",
                "params": { "name": "protected__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert!(bounded.json::<Value>().await.unwrap()["error"].is_object());
        let attempts = std::iter::from_fn(|| upstream.try_recv().ok())
            .filter(|(_, authorization)| authorization == "Bearer rejected-access-token")
            .count();
        assert!(
            attempts <= 1,
            "a rejected token must be retried at most once"
        );
        reject_oauth.store(false, std::sync::atomic::Ordering::SeqCst);
        reject_retried.store(false, std::sync::atomic::Ordering::SeqCst);

        for (code, expected_status, expected_state) in [
            ("bad-code", StatusCode::BAD_REQUEST, "error"),
            ("recovery-code", StatusCode::OK, "authorized"),
        ] {
            let restarted = client
                .post(format!(
                    "{gateway}/api/servers/{server_id}/authorization/start"
                ))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap();
            let url = reqwest::Url::parse(restarted["authorizationUrl"].as_str().unwrap()).unwrap();
            let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair("code", code)
                .append_pair("state", &query["state"])
                .append_pair("iss", &issuer);
            assert_eq!(
                client.get(callback).send().await.unwrap().status(),
                expected_status
            );
            let snapshot = client
                .get(format!("{gateway}/api/servers/{server_id}/authorization"))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap();
            assert_eq!(snapshot["state"], expected_state);
            if expected_state == "error" {
                let call = client
                    .post(format!("{gateway}/mcp"))
                    .header("accept", "application/json")
                    .header("mcp-session-id", session_id)
                    .header("mcp-protocol-version", "2025-03-26")
                    .json(&json!({
                        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                        "params": { "name": "protected__echo", "arguments": {} }
                    }))
                    .send()
                    .await
                    .unwrap();
                let body = call.json::<Value>().await.unwrap();
                assert_eq!(body["result"]["content"][0]["text"], "oauth-ok");
            }
        }

        let dynamic = client
            .post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({
                "name": "dynamic",
                "connectionType": "http",
                "url": format!("http://{upstream_addr}/mcp")
            }))
            .send()
            .await
            .unwrap();
        let dynamic_id = dynamic.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let dynamic_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let dynamic_url =
            reqwest::Url::parse(dynamic_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let dynamic_query: HashMap<String, String> =
            dynamic_url.query_pairs().into_owned().collect();
        assert_eq!(dynamic_query["client_id"], "dynamic-client-id");
        assert_eq!(dynamic_query["scope"], "mcp.read");
        assert_eq!(
            dynamic_query["resource"],
            format!("http://{upstream_addr}/mcp")
        );
        let registration = registration_forms.recv().await.unwrap();
        assert_eq!(registration["client_name"], "Moor");
        assert_eq!(
            registration["redirect_uris"][0],
            dynamic_query["redirect_uri"]
        );
        assert_eq!(
            registration["token_endpoint_auth_method"],
            "client_secret_post"
        );
        let mut dynamic_callback = reqwest::Url::parse(&dynamic_query["redirect_uri"]).unwrap();
        dynamic_callback
            .query_pairs_mut()
            .append_pair("code", "dynamic-code")
            .append_pair("state", &dynamic_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(dynamic_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        let dynamic_started = client
            .post(format!("{gateway}/api/servers/{dynamic_id}/start"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        let dynamic_started_status = dynamic_started.status();
        let dynamic_started_body = dynamic_started.text().await.unwrap();
        assert_eq!(
            dynamic_started_status,
            StatusCode::OK,
            "{dynamic_started_body}"
        );
        let dynamic_call = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "dynamic__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            dynamic_call.json::<Value>().await.unwrap()["result"]["content"][0]["text"],
            "oauth-ok"
        );
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let before = refresh_count.load(std::sync::atomic::Ordering::SeqCst);
        let authorization = state.server_manager.authorization();
        let refreshes: Vec<_> = (0..8)
            .map(|_| authorization.refresh(&dynamic_id, true))
            .collect();
        for result in futures::future::join_all(refreshes).await {
            result.unwrap();
        }
        assert_eq!(
            refresh_count.load(std::sync::atomic::Ordering::SeqCst),
            before + 1,
            "concurrent refreshes for one Server must share one AS request"
        );

        insufficient_scope.store(true, std::sync::atomic::Ordering::SeqCst);
        let before_step_up = refresh_count.load(std::sync::atomic::Ordering::SeqCst);
        let step_up = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 40, "method": "tools/call",
                "params": { "name": "dynamic__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert!(step_up.json::<Value>().await.unwrap()["error"].is_object());
        assert_eq!(
            refresh_count.load(std::sync::atomic::Ordering::SeqCst),
            before_step_up
        );
        let required = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(required["state"], "required");
        assert_eq!(required["errorCode"], "insufficient_scope");
        assert!(required["scopes"]
            .as_array()
            .unwrap()
            .contains(&json!("mcp.read")));
        assert!(required["scopes"]
            .as_array()
            .unwrap()
            .contains(&json!("mcp.write")));
        let unchanged_step_up = client
            .put(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "dynamic-client-id", "clientSecret": "dynamic-client-secret" }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert!(unchanged_step_up["scopes"]
            .as_array()
            .unwrap()
            .contains(&json!("mcp.write")));
        let step_up_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let step_up_url =
            reqwest::Url::parse(step_up_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let step_up_query: HashMap<String, String> =
            step_up_url.query_pairs().into_owned().collect();
        assert!(step_up_query["scope"].contains("mcp.read"));
        assert!(step_up_query["scope"].contains("mcp.write"));
        let mut step_up_callback = reqwest::Url::parse(&step_up_query["redirect_uri"]).unwrap();
        step_up_callback
            .query_pairs_mut()
            .append_pair("code", "step-up-code")
            .append_pair("state", &step_up_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(step_up_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        missing_scope.store(true, std::sync::atomic::Ordering::SeqCst);
        let missing = client
            .post(format!("{gateway}/mcp"))
            .header("accept", "application/json")
            .header("mcp-session-id", session_id)
            .header("mcp-protocol-version", "2025-03-26")
            .json(&json!({
                "jsonrpc": "2.0", "id": 41, "method": "tools/call",
                "params": { "name": "dynamic__echo", "arguments": {} }
            }))
            .send()
            .await
            .unwrap();
        assert!(missing.json::<Value>().await.unwrap()["error"].is_object());
        let missing_snapshot = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(missing_snapshot["state"], "required");
        insufficient_scope.store(false, std::sync::atomic::Ordering::SeqCst);
        missing_scope.store(false, std::sync::atomic::Ordering::SeqCst);

        for offline in [false, true] {
            let started = client
                .post(format!(
                    "{gateway}/api/servers/{dynamic_id}/authorization/start"
                ))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap();
            let url = reqwest::Url::parse(started["authorizationUrl"].as_str().unwrap()).unwrap();
            let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair("code", "revoke-code")
                .append_pair("state", &query["state"])
                .append_pair("iss", &issuer);
            assert_eq!(
                client.get(callback).send().await.unwrap().status(),
                StatusCode::OK
            );
            revoke_offline.store(offline, std::sync::atomic::Ordering::SeqCst);
            let revoked = client
                .delete(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap();
            assert_eq!(revoked.status(), StatusCode::OK);
            let revoked = revoked.json::<Value>().await.unwrap();
            assert_eq!(revoked["snapshot"]["state"], "required");
            assert_eq!(revoked["warning"].is_null(), !offline);
            assert!(!revoked.to_string().contains("oauth-access-token"));
            let form = revocation_forms.recv().await.unwrap();
            assert_eq!(form["client_id"], "dynamic-client-id");
            assert_eq!(form["client_secret"], "dynamic-client-secret");
            assert!(
                credentials
                    .values_for_test()
                    .iter()
                    .all(|(reference, _)| !reference.starts_with(&format!("server:{dynamic_id}:"))),
                "revocation must remove every credential for {dynamic_id}"
            );
            let remaining = state.db.query_one(
                "SELECT COUNT(*) FROM server_authorizations WHERE server_id = ?1",
                &[&dynamic_id],
                |row| row.get::<_, i64>(0),
            );
            assert_eq!(remaining, Ok(Some(1)));
        }
        revoke_offline.store(false, std::sync::atomic::Ordering::SeqCst);

        let restarted_state = AppState::for_test(&data_dir);
        let (restarted_addr, restarted_gateway) = spawn_router(create_app(restarted_state)).await;
        let persisted = client
            .get(format!(
                "http://{restarted_addr}/api/servers/{server_id}/authorization"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(persisted["state"], "authorized");
        let dynamic_persisted = client
            .get(format!(
                "http://{restarted_addr}/api/servers/{dynamic_id}/authorization"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(dynamic_persisted["state"], "required");
        assert!(dynamic_persisted["clientId"].is_null());
        assert!(!dynamic_persisted
            .to_string()
            .contains("dynamic-client-secret"));

        let (new_authorizations, mut new_upstream) = mpsc::unbounded_channel();
        let new_issuer = issuer.clone();
        let new_resource = Router::new()
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                axum::routing::get(move || {
                    let issuer = new_issuer.clone();
                    async move { Json(json!({ "authorization_servers": [issuer] })) }
                }),
            )
            .route(
                "/mcp",
                post(move |headers: HeaderMap| {
                    let new_authorizations = new_authorizations.clone();
                    async move {
                        new_authorizations
                            .send(
                                headers
                                    .get("authorization")
                                    .and_then(|value| value.to_str().ok())
                                    .unwrap_or_default()
                                    .to_string(),
                            )
                            .unwrap();
                        StatusCode::UNAUTHORIZED
                    }
                }),
            );
        let (new_addr, new_server) = spawn_router(new_resource).await;
        let pending = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let pending_url =
            reqwest::Url::parse(pending["authorizationUrl"].as_str().unwrap()).unwrap();
        let pending_query: HashMap<String, String> =
            pending_url.query_pairs().into_owned().collect();
        let mut pending_callback = reqwest::Url::parse(&pending_query["redirect_uri"]).unwrap();
        pending_callback
            .query_pairs_mut()
            .append_pair("code", "hold-code")
            .append_pair("state", &pending_query["state"])
            .append_pair("iss", &issuer);
        let callback_client = client.clone();
        let pending_callback =
            tokio::spawn(
                async move { callback_client.get(pending_callback).send().await.unwrap() },
            );
        tokio::time::timeout(Duration::from_secs(2), token_waiting.notified())
            .await
            .unwrap();
        let changed = client
            .put(format!("{gateway}/api/servers/{server_id}"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "url": format!("http://{new_addr}/mcp") }))
            .send()
            .await
            .unwrap();
        assert_eq!(changed.status(), StatusCode::OK);
        token_continue.notify_one();
        assert_eq!(
            pending_callback.await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let invalidated = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(invalidated["state"], "required");
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{server_id}:"))));
        let stopped = client
            .post(format!("{gateway}/api/servers/{server_id}/stop"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(stopped.status(), StatusCode::OK);
        let started = client
            .post(format!("{gateway}/api/servers/{server_id}/start"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(started.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(new_upstream.recv().await.unwrap(), "");

        let reauthorized = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let url = reqwest::Url::parse(reauthorized["authorizationUrl"].as_str().unwrap()).unwrap();
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("code", "new-context-code")
            .append_pair("state", &query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        let changed_client = client
            .put(format!(
                "{gateway}/api/servers/{server_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "new-client" }))
            .send()
            .await
            .unwrap();
        assert_eq!(changed_client.status(), StatusCode::OK);
        assert_eq!(
            changed_client.json::<Value>().await.unwrap()["state"],
            "required"
        );
        let current_credentials = credentials.values_for_test();
        assert!(current_credentials.iter().all(|(_, value)| ![
            "oauth-access-token",
            "oauth-refresh-token",
            "client-secret"
        ]
        .contains(&value.as_str())));
        assert!(current_credentials
            .iter()
            .all(|(_, value)| value != "dynamic-client-secret"));

        let refresh_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let refresh_url =
            reqwest::Url::parse(refresh_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let refresh_query: HashMap<String, String> =
            refresh_url.query_pairs().into_owned().collect();
        let mut refresh_callback = reqwest::Url::parse(&refresh_query["redirect_uri"]).unwrap();
        refresh_callback
            .query_pairs_mut()
            .append_pair("code", "refresh-race")
            .append_pair("state", &refresh_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(refresh_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        let failed_reference = format!("server:{dynamic_id}:oauth:refresh");
        credentials.fail_delete_for_test(Some(failed_reference.clone()));
        let partial = client
            .delete(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert!(!partial.status().is_success());
        let partial_state = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(partial_state["state"], "required");
        assert_eq!(partial_state["canRevoke"], true);
        let remaining_credentials = credentials.values_for_test();
        assert!(remaining_credentials
            .iter()
            .any(|(reference, _)| reference == &failed_reference));
        assert!(!remaining_credentials
            .iter()
            .any(|(reference, _)| reference == &format!("server:{dynamic_id}:oauth:access")));
        let disabled = client
            .post(format!("{gateway}/api/servers/{dynamic_id}/tools/refresh"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert!(!disabled.status().is_success());
        credentials.fail_delete_for_test(None);
        let retry = client
            .delete(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(retry.status(), StatusCode::OK);
        let refresh_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let refresh_url =
            reqwest::Url::parse(refresh_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let refresh_query: HashMap<String, String> =
            refresh_url.query_pairs().into_owned().collect();
        let mut refresh_callback = reqwest::Url::parse(&refresh_query["redirect_uri"]).unwrap();
        refresh_callback
            .query_pairs_mut()
            .append_pair("code", "refresh-race")
            .append_pair("state", &refresh_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(refresh_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        hold_refresh.store(true, std::sync::atomic::Ordering::SeqCst);
        reject_oauth.store(true, std::sync::atomic::Ordering::SeqCst);
        let refresh_client = client.clone();
        let refresh_endpoint = format!("{gateway}/api/servers/{dynamic_id}/tools/refresh");
        let running_refresh = tokio::spawn(async move {
            refresh_client
                .post(refresh_endpoint)
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(2), token_waiting.notified())
            .await
            .unwrap();
        let revoked_during_refresh = client
            .delete(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(revoked_during_refresh.status(), StatusCode::OK);
        token_continue.notify_one();
        assert!(!running_refresh.await.unwrap().status().is_success());
        let after_refresh = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(after_refresh["state"], "required");
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{dynamic_id}:"))));
        hold_refresh.store(false, std::sync::atomic::Ordering::SeqCst);
        reject_oauth.store(false, std::sync::atomic::Ordering::SeqCst);

        let timeout_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let timeout_url =
            reqwest::Url::parse(timeout_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let timeout_query: HashMap<String, String> =
            timeout_url.query_pairs().into_owned().collect();
        let mut timeout_callback = reqwest::Url::parse(&timeout_query["redirect_uri"]).unwrap();
        timeout_callback
            .query_pairs_mut()
            .append_pair("code", "timeout-seed")
            .append_pair("state", &timeout_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(timeout_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        revoke_hang.store(true, std::sync::atomic::Ordering::SeqCst);
        let revoke_client = client.clone();
        let revoke_endpoint = format!("{gateway}/api/servers/{dynamic_id}/authorization");
        let hanging_revoke = tokio::spawn(async move {
            revoke_client
                .delete(revoke_endpoint)
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(2), revoke_waiting.notified())
            .await
            .unwrap();
        let local_state = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(local_state["state"], "required");
        assert!(!local_state["canRevoke"].as_bool().unwrap());
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{dynamic_id}:"))));
        let timed_out = tokio::time::timeout(Duration::from_secs(6), hanging_revoke)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(timed_out.status(), StatusCode::OK);
        assert_eq!(
            timed_out.json::<Value>().await.unwrap()["warning"],
            "Provider revocation failed; local credentials were removed."
        );
        revoke_hang.store(false, std::sync::atomic::Ordering::SeqCst);

        let deletion_start = client
            .post(format!(
                "{gateway}/api/servers/{dynamic_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let deletion_url =
            reqwest::Url::parse(deletion_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let deletion_query: HashMap<String, String> =
            deletion_url.query_pairs().into_owned().collect();
        let mut deletion_callback = reqwest::Url::parse(&deletion_query["redirect_uri"]).unwrap();
        deletion_callback
            .query_pairs_mut()
            .append_pair("code", "hold-code")
            .append_pair("state", &deletion_query["state"])
            .append_pair("iss", &issuer);
        let deletion_client = client.clone();
        let deletion_callback =
            tokio::spawn(
                async move { deletion_client.get(deletion_callback).send().await.unwrap() },
            );
        tokio::time::timeout(Duration::from_secs(2), token_waiting.notified())
            .await
            .unwrap();
        let in_flight = client
            .get(format!("{gateway}/api/servers/{dynamic_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(in_flight["canRevoke"], true);
        let deleted = client
            .delete(format!("{gateway}/api/servers/{dynamic_id}"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::OK);
        token_continue.notify_one();
        assert_eq!(
            deletion_callback.await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{dynamic_id}:"))));
        let remaining = state.db.query_one(
            "SELECT COUNT(*) FROM server_authorizations WHERE server_id = ?1",
            &[&dynamic_id],
            |row| row.get::<_, i64>(0),
        );
        assert_eq!(remaining, Ok(Some(0)));

        let discovering = client.post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "name": "discovering", "connectionType": "http", "url": format!("http://{upstream_addr}/mcp") }))
            .send().await.unwrap().json::<Value>().await.unwrap();
        let discovering_id = discovering["id"].as_str().unwrap();
        let registered = client
            .put(format!(
                "{gateway}/api/servers/{discovering_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(
                &json!({ "clientId": "discovering-client", "clientSecret": "discovering-secret" }),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(registered.status(), StatusCode::OK);
        hold_discovery.store(true, std::sync::atomic::Ordering::SeqCst);
        let discovery_client = client.clone();
        let discovery_endpoint =
            format!("{gateway}/api/servers/{discovering_id}/authorization/start");
        let discovery = tokio::spawn(async move {
            discovery_client
                .post(discovery_endpoint)
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(2), discovery_waiting.notified())
            .await
            .unwrap();
        let deleted = client
            .delete(format!("{gateway}/api/servers/{discovering_id}"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::OK);
        discovery_continue.notify_one();
        assert!(!discovery.await.unwrap().status().is_success());
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{discovering_id}:"))));
        let discovering_dynamic = client.post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "name": "discovering-dynamic", "connectionType": "http", "url": format!("http://{upstream_addr}/mcp") }))
            .send().await.unwrap().json::<Value>().await.unwrap();
        let discovering_dynamic_id = discovering_dynamic["id"].as_str().unwrap();
        let discovery_client = client.clone();
        let discovery_endpoint =
            format!("{gateway}/api/servers/{discovering_dynamic_id}/authorization/start");
        let discovery = tokio::spawn(async move {
            discovery_client
                .post(discovery_endpoint)
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(2), discovery_waiting.notified())
            .await
            .unwrap();
        let pending = client
            .get(format!(
                "{gateway}/api/servers/{discovering_dynamic_id}/authorization"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(pending["state"], "authorizing");
        assert_eq!(pending["canRevoke"], true);
        let cancelled = client
            .delete(format!(
                "{gateway}/api/servers/{discovering_dynamic_id}/authorization"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(cancelled.status(), StatusCode::OK);
        discovery_continue.notify_one();
        assert!(!discovery.await.unwrap().status().is_success());
        hold_discovery.store(false, std::sync::atomic::Ordering::SeqCst);

        let query_server = client.post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "name": "query-resource", "connectionType": "http", "url": format!("http://{upstream_addr}/mcp?tenant=one") }))
            .send().await.unwrap().json::<Value>().await.unwrap();
        let query_id = query_server["id"].as_str().unwrap();
        let registered = client
            .put(format!(
                "{gateway}/api/servers/{query_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "query-client" }))
            .send()
            .await
            .unwrap();
        assert_eq!(registered.status(), StatusCode::OK);
        let query_start = client
            .post(format!(
                "{gateway}/api/servers/{query_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let query_url =
            reqwest::Url::parse(query_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let query_params: HashMap<String, String> = query_url.query_pairs().into_owned().collect();
        let mut query_callback = reqwest::Url::parse(&query_params["redirect_uri"]).unwrap();
        query_callback
            .query_pairs_mut()
            .append_pair("code", "query-code")
            .append_pair("state", &query_params["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(query_callback).send().await.unwrap().status(),
            StatusCode::OK
        );
        let query_started = client
            .post(format!("{gateway}/api/servers/{query_id}/start"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(query_started.status(), StatusCode::OK);
        let changed_query = client
            .put(format!("{gateway}/api/servers/{query_id}"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "url": format!("http://{upstream_addr}/mcp?tenant=two") }))
            .send()
            .await
            .unwrap();
        assert_eq!(changed_query.status(), StatusCode::OK);
        let invalidated_query = client
            .get(format!("{gateway}/api/servers/{query_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(invalidated_query["state"], "required");
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{query_id}:"))));
        let query_reauth = client
            .post(format!(
                "{gateway}/api/servers/{query_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let query_reauth_url =
            reqwest::Url::parse(query_reauth["authorizationUrl"].as_str().unwrap()).unwrap();
        let query_reauth_params: HashMap<String, String> =
            query_reauth_url.query_pairs().into_owned().collect();
        let mut query_reauth_callback =
            reqwest::Url::parse(&query_reauth_params["redirect_uri"]).unwrap();
        query_reauth_callback
            .query_pairs_mut()
            .append_pair("code", "query-reauth")
            .append_pair("state", &query_reauth_params["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client
                .get(query_reauth_callback)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let stale_session = client
            .post(format!("{gateway}/api/servers/{query_id}/tools/refresh"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert!(!stale_session.status().is_success());

        let write_failure = client.post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({ "name": "write-failure", "connectionType": "http", "url": format!("http://{upstream_addr}/mcp") }))
            .send().await.unwrap().json::<Value>().await.unwrap();
        let write_id = write_failure["id"].as_str().unwrap();
        let registered = client
            .put(format!(
                "{gateway}/api/servers/{write_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "write-client" }))
            .send()
            .await
            .unwrap();
        assert_eq!(registered.status(), StatusCode::OK);
        credentials.fail_set_for_test(Some(format!("server:{write_id}:oauth:refresh")));
        let write_start = client
            .post(format!(
                "{gateway}/api/servers/{write_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let write_url =
            reqwest::Url::parse(write_start["authorizationUrl"].as_str().unwrap()).unwrap();
        let write_query: HashMap<String, String> = write_url.query_pairs().into_owned().collect();
        let mut write_callback = reqwest::Url::parse(&write_query["redirect_uri"]).unwrap();
        write_callback
            .query_pairs_mut()
            .append_pair("code", "write-failure")
            .append_pair("state", &write_query["state"])
            .append_pair("iss", &issuer);
        assert_eq!(
            client.get(write_callback).send().await.unwrap().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        credentials.fail_set_for_test(None);
        let write_deleted = client
            .delete(format!("{gateway}/api/servers/{write_id}"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(write_deleted.status(), StatusCode::OK);
        assert!(credentials
            .values_for_test()
            .iter()
            .all(|(reference, _)| !reference.starts_with(&format!("server:{write_id}:"))));

        new_server.abort();
        restarted_gateway.abort();
        gateway_server.abort();
        upstream_server.abort();
        as_server.abort();
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn oauth_credential_store_failure_is_explicit_and_does_not_persist_secrets() {
        let data_dir =
            std::env::temp_dir().join(format!("moor-oauth-unavailable-{}", uuid::Uuid::new_v4()));
        let state = AppState::for_test_with_credential_store(
            &data_dir,
            Arc::new(
                crate::sidecar::services::server_authorization::MemoryCredentialStore::unavailable(
                ),
            ),
        );
        let (gateway_addr, gateway_server) = spawn_router(create_app(state.clone())).await;
        let gateway = format!("http://{gateway_addr}");
        let client = reqwest::Client::new();
        let create = client
            .post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({
                "name": "protected",
                "connectionType": "http",
                "url": "https://mcp.example.com/mcp"
            }))
            .send()
            .await
            .unwrap();
        let server_id = create.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let response = client
            .put(format!(
                "{gateway}/api/servers/{server_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "moor-test-client", "clientSecret": "client-secret" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let snapshot = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(snapshot["state"], "not_required");
        assert!(!snapshot.to_string().contains("client-secret"));
        let count = state
            .db
            .query_one("SELECT COUNT(*) FROM server_authorizations", &[], |row| {
                row.get::<_, i64>(0)
            });
        assert_eq!(count, Ok(Some(0)));

        gateway_server.abort();
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn oauth_rejects_authorization_servers_without_pkce() {
        let issuer_for_metadata = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let metadata_issuer = issuer_for_metadata.clone();
        let authorization_server = Router::new().route(
            "/.well-known/oauth-authorization-server",
            axum::routing::get(move || {
                let issuer = metadata_issuer.lock().unwrap().clone();
                async move {
                    Json(json!({
                        "issuer": issuer,
                        "authorization_endpoint": "http://127.0.0.1/authorize",
                        "token_endpoint": "http://127.0.0.1/token"
                    }))
                }
            }),
        );
        let (as_addr, as_server) = spawn_router(authorization_server).await;
        *issuer_for_metadata.lock().unwrap() = format!("http://{as_addr}");
        let issuer = format!("http://{as_addr}");
        let protected = Router::new()
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                axum::routing::get(move || {
                    let issuer = issuer.clone();
                    async move { Json(json!({ "authorization_servers": [issuer] })) }
                }),
            )
            .route("/mcp", post(|| async { StatusCode::UNAUTHORIZED }));
        let (upstream_addr, upstream_server) = spawn_router(protected).await;
        let data_dir =
            std::env::temp_dir().join(format!("moor-oauth-pkce-{}", uuid::Uuid::new_v4()));
        let state = AppState::for_test(&data_dir);
        let (gateway_addr, gateway_server) = spawn_router(create_app(state)).await;
        let gateway = format!("http://{gateway_addr}");
        let client = reqwest::Client::new();
        let create = client
            .post(format!("{gateway}/api/servers"))
            .header("x-moor-token", "test-token")
            .json(&json!({
                "name": "protected", "connectionType": "http",
                "url": format!("http://{upstream_addr}/mcp")
            }))
            .send()
            .await
            .unwrap();
        let server_id = create.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        client
            .put(format!(
                "{gateway}/api/servers/{server_id}/authorization/client"
            ))
            .header("x-moor-token", "test-token")
            .json(&json!({ "clientId": "moor-test-client" }))
            .send()
            .await
            .unwrap();
        let started = client
            .post(format!(
                "{gateway}/api/servers/{server_id}/authorization/start"
            ))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(started.status(), StatusCode::BAD_GATEWAY);
        let snapshot = client
            .get(format!("{gateway}/api/servers/{server_id}/authorization"))
            .header("x-moor-token", "test-token")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(snapshot["state"], "error");
        assert_eq!(snapshot["errorCode"], "pkce_unsupported");
        assert!(snapshot["errorMessage"]
            .as_str()
            .unwrap()
            .contains("PKCE S256"));

        gateway_server.abort();
        upstream_server.abort();
        as_server.abort();
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn oidc_discovery_variants_complete_dynamic_authorization() {
        for (issuer_path, metadata_path) in [
            (
                "/tenant1",
                "/.well-known/oauth-authorization-server/tenant1",
            ),
            ("/tenant1", "/.well-known/openid-configuration/tenant1"),
            ("/tenant1", "/tenant1/.well-known/openid-configuration"),
            ("", "/.well-known/openid-configuration"),
        ] {
            let issuer_slot = Arc::new(std::sync::Mutex::new(String::new()));
            let metadata_issuer = issuer_slot.clone();
            let authorization_server = Router::new()
                .route(
                    metadata_path,
                    axum::routing::get(move |headers: HeaderMap| {
                        let issuer = metadata_issuer.lock().unwrap().clone();
                        async move {
                            let base = format!(
                                "http://{}",
                                headers.get("host").unwrap().to_str().unwrap()
                            );
                            Json(json!({
                                "issuer": issuer,
                                "authorization_endpoint": format!("{base}/authorize"),
                                "token_endpoint": format!("{base}/token"),
                                "registration_endpoint": format!("{base}/register"),
                                "code_challenge_methods_supported": ["S256"],
                                "authorization_response_iss_parameter_supported": true
                            }))
                        }
                    }),
                )
                .route(
                    "/register",
                    post(|| async {
                        Json(json!({
                            "client_id": "oidc-dynamic-client",
                            "client_secret": "oidc-dynamic-secret"
                        }))
                    }),
                )
                .route(
                    "/token",
                    post(|| async {
                        Json(json!({
                            "access_token": "oidc-access-token",
                            "token_type": "Bearer",
                            "expires_in": 3600
                        }))
                    }),
                );
            let (as_addr, as_server) = spawn_router(authorization_server).await;
            let issuer = format!("http://{as_addr}{issuer_path}");
            *issuer_slot.lock().unwrap() = issuer.clone();
            let resource_issuer = issuer.clone();
            let protected = Router::new()
                .route(
                    "/custom-resource",
                    axum::routing::get(move || {
                        let issuer = resource_issuer.clone();
                        async move {
                            Json(json!({
                                "authorization_servers": [issuer],
                                "scopes_supported": ["mcp.read"]
                            }))
                        }
                    }),
                )
                .route(
                    "/mcp",
                    post(|headers: HeaderMap| async move {
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        (
                            StatusCode::UNAUTHORIZED,
                            [(
                                "www-authenticate",
                                format!(
                                    "Bearer resource_metadata=\"http://{host}/custom-resource\""
                                ),
                            )],
                        )
                    }),
                );
            let (upstream_addr, upstream_server) = spawn_router(protected).await;
            let data_dir = std::env::temp_dir().join(format!("moor-oidc-{}", uuid::Uuid::new_v4()));
            let state = AppState::for_test(&data_dir);
            let (gateway_addr, gateway_server) = spawn_router(create_app(state)).await;
            let gateway = format!("http://{gateway_addr}");
            let client = reqwest::Client::new();
            let created = client
                .post(format!("{gateway}/api/servers"))
                .header("x-moor-token", "test-token")
                .json(&json!({
                    "name": "oidc", "connectionType": "http",
                    "url": format!("http://{upstream_addr}/mcp")
                }))
                .send()
                .await
                .unwrap();
            let server_id = created.json::<Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let preregistered = client
                .put(format!(
                    "{gateway}/api/servers/{server_id}/authorization/client"
                ))
                .header("x-moor-token", "test-token")
                .json(&json!({
                    "clientId": "old-manual-client",
                    "clientSecret": "old-manual-secret"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(preregistered.status(), StatusCode::OK);
            let started = client
                .post(format!(
                    "{gateway}/api/servers/{server_id}/authorization/start"
                ))
                .header("x-moor-token", "test-token")
                .json(&json!({ "clientId": "" }))
                .send()
                .await
                .unwrap();
            assert_eq!(started.status(), StatusCode::OK, "{metadata_path}");
            let started = started.json::<Value>().await.unwrap();
            let url = reqwest::Url::parse(started["authorizationUrl"].as_str().unwrap()).unwrap();
            let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
            assert_eq!(query["client_id"], "oidc-dynamic-client");
            assert_eq!(query["scope"], "mcp.read");
            assert_eq!(query["resource"], format!("http://{upstream_addr}/mcp"));
            let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair("code", "oidc-code")
                .append_pair("state", &query["state"])
                .append_pair("iss", &issuer);
            assert_eq!(
                client.get(callback).send().await.unwrap().status(),
                StatusCode::OK,
                "{metadata_path}"
            );
            let snapshot = client
                .get(format!("{gateway}/api/servers/{server_id}/authorization"))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap();
            assert_eq!(snapshot["state"], "authorized");
            assert_eq!(snapshot["clientId"], "oidc-dynamic-client");
            assert!(!snapshot.to_string().contains("oidc-dynamic-secret"));

            gateway_server.abort();
            upstream_server.abort();
            as_server.abort();
            let _ = std::fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn malformed_metadata_and_missing_dcr_fall_back_safely() {
        for malformed in [true, false] {
            let issuer_slot = Arc::new(std::sync::Mutex::new(String::new()));
            let metadata_issuer = issuer_slot.clone();
            let authorization_server = Router::new()
                .route(
                    "/.well-known/oauth-authorization-server",
                    axum::routing::get(move |headers: HeaderMap| {
                        let issuer = metadata_issuer.lock().unwrap().clone();
                        async move {
                            if malformed {
                                return Json(json!({ "issuer": issuer }));
                            }
                            let base = format!(
                                "http://{}",
                                headers.get("host").unwrap().to_str().unwrap()
                            );
                            Json(json!({
                                "issuer": issuer,
                                "authorization_endpoint": format!("{base}/authorize"),
                                "token_endpoint": format!("{base}/token"),
                                "code_challenge_methods_supported": ["S256"],
                                "authorization_response_iss_parameter_supported": true,
                                "client_id_metadata_document_supported": true
                            }))
                        }
                    }),
                )
                .route(
                    "/token",
                    post(|| async {
                        Json(json!({
                            "access_token": "fallback-access-token",
                            "token_type": "Bearer"
                        }))
                    }),
                );
            let (as_addr, as_server) = spawn_router(authorization_server).await;
            *issuer_slot.lock().unwrap() = format!("http://{as_addr}");
            let issuer = format!("http://{as_addr}");
            let resource_issuer = issuer.clone();
            let protected = Router::new()
                .route(
                    "/.well-known/oauth-protected-resource/mcp",
                    axum::routing::get(move || {
                        let issuer = resource_issuer.clone();
                        async move { Json(json!({ "authorization_servers": [issuer] })) }
                    }),
                )
                .route("/mcp", post(|| async { StatusCode::UNAUTHORIZED }));
            let (upstream_addr, upstream_server) = spawn_router(protected).await;
            let data_dir =
                std::env::temp_dir().join(format!("moor-oauth-fallback-{}", uuid::Uuid::new_v4()));
            let state = AppState::for_test(&data_dir);
            let (gateway_addr, gateway_server) = spawn_router(create_app(state)).await;
            let gateway = format!("http://{gateway_addr}");
            let client = reqwest::Client::new();
            let created = client
                .post(format!("{gateway}/api/servers"))
                .header("x-moor-token", "test-token")
                .json(&json!({
                    "name": "fallback", "connectionType": "http",
                    "url": format!("http://{upstream_addr}/mcp")
                }))
                .send()
                .await
                .unwrap();
            let server_id = created.json::<Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let start = client
                .post(format!(
                    "{gateway}/api/servers/{server_id}/authorization/start"
                ))
                .header("x-moor-token", "test-token")
                .send()
                .await
                .unwrap();
            assert_eq!(start.status(), StatusCode::BAD_GATEWAY);
            if malformed {
                let snapshot = client
                    .get(format!("{gateway}/api/servers/{server_id}/authorization"))
                    .header("x-moor-token", "test-token")
                    .send()
                    .await
                    .unwrap()
                    .json::<Value>()
                    .await
                    .unwrap();
                assert_eq!(snapshot["state"], "error");
                assert!(snapshot["errorMessage"]
                    .as_str()
                    .unwrap()
                    .contains("metadata"));
            } else {
                client
                    .put(format!(
                        "{gateway}/api/servers/{server_id}/authorization/client"
                    ))
                    .header("x-moor-token", "test-token")
                    .json(&json!({ "clientId": "manual-fallback-client" }))
                    .send()
                    .await
                    .unwrap();
                let start = client
                    .post(format!(
                        "{gateway}/api/servers/{server_id}/authorization/start"
                    ))
                    .header("x-moor-token", "test-token")
                    .send()
                    .await
                    .unwrap();
                assert_eq!(start.status(), StatusCode::OK);
                let started = start.json::<Value>().await.unwrap();
                let url =
                    reqwest::Url::parse(started["authorizationUrl"].as_str().unwrap()).unwrap();
                let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
                assert_eq!(query["client_id"], "manual-fallback-client");
                assert!(!query["client_id"].starts_with("https://"));
                let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
                callback
                    .query_pairs_mut()
                    .append_pair("code", "fallback-code")
                    .append_pair("state", &query["state"])
                    .append_pair("iss", &issuer);
                assert_eq!(
                    client.get(callback).send().await.unwrap().status(),
                    StatusCode::OK
                );
            }

            gateway_server.abort();
            upstream_server.abort();
            as_server.abort();
            let _ = std::fs::remove_dir_all(data_dir);
        }
    }
}
