pub mod app_error;
mod auth;
pub mod routes;

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
            Arc::new(ServerManager::new(db, event_bus)),
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
}
