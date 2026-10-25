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
                "/token",
                post(
                    move |axum::Form(form): axum::Form<HashMap<String, String>>| {
                        let token_requests = token_requests.clone();
                        async move {
                            token_requests.send(form.clone()).unwrap();
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
                    async move {
                        let authorization = headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_string();
                        if authorization.is_empty() {
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
        let state = AppState::for_test(&data_dir);
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
        assert_eq!(abandoned["state"], "authorized");
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
                assert_eq!(
                    call.json::<Value>().await.unwrap()["result"]["content"][0]["text"],
                    "oauth-ok"
                );
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
        assert_eq!(dynamic_started.status(), StatusCode::OK);
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
        assert_eq!(dynamic_persisted["state"], "authorized");
        assert_eq!(dynamic_persisted["clientId"], "dynamic-client-id");
        assert!(!dynamic_persisted
            .to_string()
            .contains("dynamic-client-secret"));

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
            let started = client
                .post(format!(
                    "{gateway}/api/servers/{server_id}/authorization/start"
                ))
                .header("x-moor-token", "test-token")
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
