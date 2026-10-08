use super::AppState;
use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

const ALLOWED_DEV_ORIGINS: &[&str] = &[
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

fn is_loopback_host(host: &str) -> bool {
    if host == "[::1]" || host.starts_with("[::1]:") {
        return true;
    }
    let hostname = host.split(':').next().unwrap_or("").to_lowercase();
    matches!(hostname.as_str(), "127.0.0.1" | "localhost" | "::1")
}

// RFC1918 only — accepted for /mcp when allowLanMcpAccess is on; /api/* stays loopback-only (#60)
fn is_private_host(host: &str) -> bool {
    host.split(':')
        .next()
        .unwrap_or("")
        .parse::<std::net::Ipv4Addr>()
        .is_ok_and(|ip| ip.is_private())
}

fn is_allowed_origin(origin: &str) -> bool {
    ALLOWED_DEV_ORIGINS.contains(&origin)
}

pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();

    // Host check — /mcp additionally accepts RFC1918 hosts (WSL2/LAN, #60);
    // unreachable unless the gateway is bound to 0.0.0.0 via allowLanMcpAccess.
    let path = req.uri().path();
    let mcp_path = path == "/mcp";
    if let Some(host) = headers.get(header::HOST) {
        let host = host.to_str().unwrap_or("");
        let host_ok = is_loopback_host(host) || (mcp_path && is_private_host(host));
        if !host_ok {
            return super::json_error_response(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "Invalid Host header",
            );
        }
    }

    // Origin check
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(ref origin) = origin {
        if !is_allowed_origin(origin) {
            return super::json_error_response(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "Invalid Origin header",
            );
        }
    }

    // CORS preflight
    if req.method() == "OPTIONS" {
        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        if let Some(ref origin) = origin {
            response
                .headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
        }
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "Content-Type, X-Moor-Token, Mcp-Session-Id, Mcp-Protocol-Version, Mcp-Method, Mcp-Name"
                .parse()
                .unwrap(),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            "GET, POST, PUT, PATCH, DELETE, OPTIONS".parse().unwrap(),
        );
        return response;
    }

    // Token check for /api/* paths — before running handler
    if path.starts_with("/api/") {
        let token = headers.get("x-moor-token").and_then(|v| v.to_str().ok());
        match token {
            Some(t) if t == state.api_token => {}
            _ => {
                return super::json_error_response(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "Unauthorized",
                )
            }
        }
    }

    // Remove host check headers to avoid passing them to handler
    let response = next.run(req).await;

    // Add CORS headers to response
    let mut response = response;
    if let Some(ref origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    }
    response
        .headers_mut()
        .insert(header::VARY, "Origin".parse().unwrap());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tauri_substring_origins() {
        assert!(!is_allowed_origin("https://evil-tauri.example"));
    }

    #[test]
    fn allows_exact_tauri_and_dev_origins() {
        assert!(is_allowed_origin("tauri://localhost"));
        assert!(is_allowed_origin("http://localhost:1420"));
    }

    #[test]
    fn recognizes_loopback_hosts() {
        assert!(is_loopback_host("127.0.0.1:9223"));
        assert!(is_loopback_host("[::1]:9223"));
        assert!(!is_loopback_host("192.168.1.10:9223"));
    }

    #[test]
    fn recognizes_rfc1918_hosts_only() {
        assert!(is_private_host("10.0.0.5:9223"));
        assert!(is_private_host("172.26.32.1:9223"));
        assert!(is_private_host("192.168.1.10:9223"));
        assert!(!is_private_host("8.8.8.8:9223"));
        assert!(!is_private_host("localhost:9223"));
        assert!(!is_private_host("172.32.0.1:9223"));
    }

    #[tokio::test]
    async fn lan_listener_accepts_private_mcp_hosts_and_keeps_api_loopback_only() {
        let data_dir = std::env::temp_dir().join(format!("moor-lan-{}", uuid::Uuid::new_v4()));
        let state = AppState::for_test(&data_dir);
        let listener = crate::bind_listener("0.0.0.0", 0).expect("bind listener");
        let address = listener.local_addr().unwrap();
        assert!(address.ip().is_unspecified());
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let (shutdown, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, crate::sidecar::http::create_app(state))
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .expect("serve test gateway");
        });
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mcp", address.port());
        for host in ["10.0.0.5", "172.16.0.5", "192.168.1.5", "8.8.8.8"] {
            let response = client.post(&url)
                .header("Host", format!("{host}:{}", address.port()))
                .header("Accept", "application/json")
                .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "LAN test", "version": "1"}}}))
                .send().await.unwrap();
            let status = response.status();
            let value: serde_json::Value = response.json().await.unwrap();
            if host == "8.8.8.8" {
                assert_eq!(status, StatusCode::FORBIDDEN);
                assert_eq!(value["error"]["message"], "Invalid Host header");
            } else {
                assert_eq!(status, StatusCode::OK);
                assert!(value["result"]["serverInfo"].is_object());
            }
        }
        let response = client
            .get(format!("http://127.0.0.1:{}/api/health", address.port()))
            .header("Host", format!("192.168.1.5:{}", address.port()))
            .header("X-Moor-Token", "test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let _: serde_json::Value = response.json().await.unwrap();
        shutdown.send(()).unwrap();
        server.await.unwrap();
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
