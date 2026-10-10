use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;

use super::format_timeout_duration;
use super::request_error::{parse_response, RequestError};

static ENV_PATTERN: OnceLock<regex_lite::Regex> = OnceLock::new();
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";
const REMOTE_MCP_ERROR_PREFIX: &str = "Remote MCP server error: ";

/// MCP Client over HTTP transport (Streamable HTTP or SSE).
/// Uses reqwest to communicate with HTTP-based MCP servers.
pub struct HttpClientTransport {
    url: String,
    headers: HashMap<String, String>,
    client: reqwest::Client,
    mode: Mutex<HttpMode>,
    session_id: Mutex<Option<String>>,
    request_timeout: Duration,
}

enum HttpMode {
    Unknown,
    Streamable,
    Sse(SseState),
}

struct SseState {
    endpoint: String,
    response: reqwest::Response,
    buffer: String,
}

struct SseEvent {
    event: Option<String>,
    data: String,
}

enum StreamableError {
    Unsupported(String),
    Failed(RequestError),
}

impl HttpClientTransport {
    pub fn new(url: &str, headers: HashMap<String, String>, request_timeout: Duration) -> Self {
        Self {
            url: url.to_string(),
            headers,
            client: reqwest::Client::new(),
            mode: Mutex::new(HttpMode::Unknown),
            session_id: Mutex::new(None),
            request_timeout,
        }
    }

    pub fn set_request_timeout(&mut self, request_timeout: Duration) {
        self.request_timeout = request_timeout;
    }

    /// Send a JSON-RPC request and get the response.
    /// First attempts Streamable HTTP, falls back to SSE on failure.
    pub async fn send_request(
        &self,
        id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, String> {
        self.send_request_with_timeout(id, method, params, self.request_timeout)
            .await
            .map_err(|error| error.message)
    }

    pub async fn send_request_with_timeout(
        &self,
        id: i64,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, RequestError> {
        tokio::time::timeout(timeout, self.send_request_inner(id, method, params))
            .await
            .map_err(|_| {
                RequestError::from(format!(
                    "HTTP request timed out after {}",
                    format_timeout_duration(timeout)
                ))
            })?
    }

    async fn send_request_inner(
        &self,
        id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, RequestError> {
        let mut mode = self.mode.lock().await;
        match &mut *mode {
            HttpMode::Streamable => self
                .send_streamable_request(id, method, params)
                .await
                .map_err(StreamableError::into_request_error),
            HttpMode::Sse(state) => self.send_sse_request(state, id, method, params).await,
            HttpMode::Unknown => match self
                .send_streamable_request(id, method, params.clone())
                .await
            {
                Ok(value) => {
                    *mode = HttpMode::Streamable;
                    Ok(value)
                }
                Err(StreamableError::Unsupported(_)) => {
                    let mut state = self.open_sse().await?;
                    let value = self
                        .send_sse_request(&mut state, id, method, params)
                        .await?;
                    *mode = HttpMode::Sse(state);
                    Ok(value)
                }
                Err(StreamableError::Failed(message)) => Err(message),
            },
        }
    }

    async fn send_streamable_request(
        &self,
        id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, StreamableError> {
        let mut request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        });
        if let Some(params) = params {
            request_body["params"] = params;
        }

        let builder = self
            .streamable_post_builder()
            .await
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2024-11-05");

        let response = builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| StreamableError::Unsupported(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            if let Some(error) = remote_jsonrpc_error(response).await {
                return Err(StreamableError::Failed(error));
            }
            return if is_streamable_unsupported_status(status) {
                Err(StreamableError::Unsupported(format!(
                    "Streamable HTTP unsupported: {status}"
                )))
            } else {
                Err(StreamableError::Failed(
                    format!("HTTP request failed: {status}").into(),
                ))
            };
        }

        self.capture_streamable_session_id(method, response.headers())
            .await;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("text/event-stream") {
            let mut response = response;
            let mut buffer = String::new();
            read_sse_jsonrpc_response(&mut response, &mut buffer, Some(id), self.request_timeout)
                .await
                .map_err(StreamableError::Failed)
        } else {
            let body = response.json::<Value>().await.map_err(|e| {
                StreamableError::Unsupported(format!(
                    "Failed to parse JSON response ({status}): {e}"
                ))
            })?;

            parse_response(&body).map_err(StreamableError::Failed)
        }
    }

    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), String> {
        let mut mode = self.mode.lock().await;
        match &mut *mode {
            HttpMode::Streamable => self
                .send_streamable_notification(method, params)
                .await
                .map_err(StreamableError::into_message),
            HttpMode::Sse(state) => self.send_sse_notification(state, method, params).await,
            HttpMode::Unknown => match self
                .send_streamable_notification(method, params.clone())
                .await
            {
                Ok(()) => {
                    *mode = HttpMode::Streamable;
                    Ok(())
                }
                Err(StreamableError::Unsupported(_)) => {
                    let mut state = self.open_sse().await?;
                    self.send_sse_notification(&mut state, method, params)
                        .await?;
                    *mode = HttpMode::Sse(state);
                    Ok(())
                }
                Err(StreamableError::Failed(error)) => Err(error.message),
            },
        }
    }

    async fn send_streamable_notification(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), StreamableError> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });

        let response = self
            .streamable_post_builder()
            .await
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2024-11-05")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| StreamableError::Unsupported(format!("HTTP notification failed: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            if let Some(message) = remote_jsonrpc_error_message(response).await {
                return Err(StreamableError::Failed(
                    format!("{REMOTE_MCP_ERROR_PREFIX}{message}").into(),
                ));
            }
            return if is_streamable_unsupported_status(status) {
                Err(StreamableError::Unsupported(format!(
                    "Streamable HTTP unsupported: {status}"
                )))
            } else {
                Err(StreamableError::Failed(
                    format!("HTTP notification failed: {status}").into(),
                ))
            };
        }
        Ok(())
    }

    async fn streamable_post_builder(&self) -> reqwest::RequestBuilder {
        let session_id = self.session_id.lock().await.clone();
        let mut builder = self.client.post(&self.url);
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        if let Some(session_id) = session_id {
            builder = builder.header(MCP_SESSION_ID_HEADER, session_id);
        }
        builder
    }

    async fn capture_streamable_session_id(
        &self,
        method: &str,
        headers: &reqwest::header::HeaderMap,
    ) {
        if method != "initialize" {
            return;
        }
        let Some(session_id) = headers
            .get(MCP_SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        *self.session_id.lock().await = Some(session_id.to_string());
    }

    async fn open_sse(&self) -> Result<SseState, String> {
        let mut builder = self.client.get(&self.url);
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        let response = builder
            .header("accept", "text/event-stream")
            .header("mcp-protocol-version", "2024-11-05")
            .send()
            .await
            .map_err(|e| format!("SSE connection failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("SSE connection failed: {}", response.status()));
        }

        let mut state = SseState {
            endpoint: String::new(),
            response,
            buffer: String::new(),
        };
        loop {
            let event =
                read_next_sse_event(&mut state.response, &mut state.buffer, self.request_timeout)
                    .await?;
            if event.event.as_deref() == Some("endpoint") {
                state.endpoint = resolve_sse_endpoint(&self.url, &event.data)?;
                return Ok(state);
            }
        }
    }

    async fn send_sse_request(
        &self,
        state: &mut SseState,
        id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, RequestError> {
        let mut request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        });
        if let Some(params) = params {
            request_body["params"] = params;
        }
        self.post_sse_message(state, &request_body).await?;
        read_sse_jsonrpc_response(
            &mut state.response,
            &mut state.buffer,
            Some(id),
            self.request_timeout,
        )
        .await
    }

    async fn send_sse_notification(
        &self,
        state: &mut SseState,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), String> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });
        self.post_sse_message(state, &request_body).await
    }

    async fn post_sse_message(&self, state: &SseState, message: &Value) -> Result<(), String> {
        let mut builder = self.client.post(&state.endpoint);
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        let response = builder
            .header("accept", "application/json")
            .header("mcp-protocol-version", "2024-11-05")
            .json(message)
            .send()
            .await
            .map_err(|e| format!("SSE message post failed: {e}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("SSE message post failed: {}", response.status()))
        }
    }
}

impl StreamableError {
    fn into_request_error(self) -> RequestError {
        match self {
            Self::Unsupported(message) => message.into(),
            Self::Failed(error) => error,
        }
    }

    fn into_message(self) -> String {
        self.into_request_error().message
    }
}

async fn remote_jsonrpc_error(response: reqwest::Response) -> Option<RequestError> {
    let body = response.json::<Value>().await.ok()?;
    let mut error = RequestError::from_rpc(body.get("error")?);
    error.message = format!("{REMOTE_MCP_ERROR_PREFIX}{}", error.message);
    Some(error)
}

fn is_streamable_unsupported_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 400 | 404 | 405 | 406 | 415)
}

async fn remote_jsonrpc_error_message(response: reqwest::Response) -> Option<String> {
    let body = response.json::<Value>().await.ok()?;
    jsonrpc_error_message(&body).map(String::from)
}

fn jsonrpc_error_message(body: &Value) -> Option<&str> {
    body.get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
}

async fn read_sse_jsonrpc_response(
    response: &mut reqwest::Response,
    buffer: &mut String,
    expected_id: Option<i64>,
    idle_timeout: Duration,
) -> Result<Value, RequestError> {
    loop {
        let event = read_next_sse_event(response, buffer, idle_timeout).await?;
        if event
            .event
            .as_deref()
            .is_some_and(|event| event != "message")
        {
            continue;
        }
        let parsed: Value =
            serde_json::from_str(&event.data).map_err(|e| format!("Invalid SSE data JSON: {e}"))?;
        if let Some(expected_id) = expected_id {
            if parsed.get("id").and_then(|v| v.as_i64()) != Some(expected_id) {
                continue;
            }
        }
        return parse_response(&parsed);
    }
}

async fn read_next_sse_event(
    response: &mut reqwest::Response,
    buffer: &mut String,
    idle_timeout: Duration,
) -> Result<SseEvent, String> {
    loop {
        if let Some(event) = take_sse_event(buffer) {
            return Ok(event);
        }
        let chunk = tokio::time::timeout(idle_timeout, response.chunk())
            .await
            .map_err(|_| {
                format!(
                    "SSE response timed out after {}",
                    format_timeout_duration(idle_timeout)
                )
            })?
            .map_err(|e| format!("Failed to read SSE response: {e}"))?;
        let Some(chunk) = chunk else {
            return Err("SSE stream closed before a response event".to_string());
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
    }
}

fn take_sse_event(buffer: &mut String) -> Option<SseEvent> {
    loop {
        let (index, delimiter_len) = find_sse_event_boundary(buffer)?;
        let raw = buffer[..index].to_string();
        buffer.drain(..index + delimiter_len);
        if let Some(event) = parse_sse_event(&raw) {
            return Some(event);
        }
    }
}

fn find_sse_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    [("\r\n\r\n", 4), ("\n\n", 2)]
        .into_iter()
        .filter_map(|(delimiter, len)| buffer.find(delimiter).map(|index| (index, len)))
        .min_by_key(|(index, _)| *index)
}

fn parse_sse_event(raw: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data = Vec::new();
    for line in raw.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        }
    }
    if data.is_empty() {
        None
    } else {
        Some(SseEvent {
            event,
            data: data.join("\n"),
        })
    }
}

fn resolve_sse_endpoint(base_url: &str, endpoint: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(base_url).map_err(|e| format!("Invalid SSE URL: {e}"))?;
    let resolved = parsed
        .join(endpoint)
        .map_err(|e| format!("Invalid SSE endpoint: {e}"))?;
    if parsed.scheme() != resolved.scheme()
        || parsed.host_str() != resolved.host_str()
        || parsed.port_or_known_default() != resolved.port_or_known_default()
    {
        return Err("SSE endpoint must be on the same origin as the configured URL".to_string());
    }
    Ok(resolved.to_string())
}

/// Resolve header values that may contain `{env:VAR_NAME}` patterns.
pub fn resolve_http_headers(
    headers: Option<&HashMap<String, String>>,
    env: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    resolve_http_headers_with_process_env(headers, env, |name| std::env::var(name).ok())
}

fn resolve_http_headers_with_process_env<F>(
    headers: Option<&HashMap<String, String>>,
    env: Option<&HashMap<String, String>>,
    process_env: F,
) -> HashMap<String, String>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(headers) = headers else {
        return HashMap::new();
    };
    headers
        .iter()
        .filter_map(|(k, v)| {
            let resolved = resolve_header_value_with_process_env(v, env, &process_env)?;
            Some((k.clone(), resolved))
        })
        .collect()
}

fn resolve_header_value_with_process_env<F>(
    value: &str,
    env: Option<&HashMap<String, String>>,
    process_env: &F,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut missing = false;
    let re = ENV_PATTERN.get_or_init(|| {
        regex_lite::Regex::new(r"\{env:([A-Za-z_][A-Za-z0-9_]*)\}")
            .expect("static env placeholder regex must compile")
    });
    let resolved = re
        .replace_all(value, |caps: &regex_lite::Captures| {
            let var_name = &caps[1];
            if let Some(val) = env
                .and_then(|server_env| server_env.get(var_name))
                .cloned()
                .or_else(|| process_env(var_name))
            {
                return val;
            }
            missing = true;
            String::new()
        })
        .to_string();

    if missing {
        None
    } else {
        Some(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::mcp::transport::mcp_client::{HttpConnectConfig, McpClient};
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{sse::Event, IntoResponse, Sse},
        routing::{get, post},
        Json, Router,
    };
    use futures::{stream::Stream, StreamExt};
    use std::convert::Infallible;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex};

    #[derive(Clone)]
    struct SseTestState {
        sender: mpsc::UnboundedSender<Value>,
        receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<Value>>>>,
    }

    async fn sse_endpoint(
        State(state): State<SseTestState>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let receiver = state
            .receiver
            .lock()
            .await
            .take()
            .expect("test opens one SSE stream");
        let endpoint = futures::stream::once(async {
            Ok(Event::default().event("endpoint").data("/messages"))
        });
        let messages =
            tokio_stream::wrappers::UnboundedReceiverStream::new(receiver).map(|value| {
                Ok(Event::default()
                    .event("message")
                    .data(serde_json::to_string(&value).expect("json response should serialize")))
            });
        Sse::new(endpoint.chain(messages))
    }

    async fn message_endpoint(
        State(state): State<SseTestState>,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        state
            .sender
            .send(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "ok": true }
            }))
            .expect("sse response should be queued");
        StatusCode::ACCEPTED
    }

    async fn delayed_progress_message_endpoint(
        State(state): State<SseTestState>,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let sender = state.sender.clone();
        tokio::spawn(async move {
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = sender.send(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": -1,
                    "result": { "keepalive": true }
                }));
            }
            let _ = sender.send(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "ok": true }
            }));
        });
        StatusCode::ACCEPTED
    }

    async fn streamable_unsupported() -> impl IntoResponse {
        StatusCode::METHOD_NOT_ALLOWED
    }

    async fn delayed_streamable_json(Json(body): Json<Value>) -> impl IntoResponse {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "ok": true }
        }))
    }

    async fn streamable_session_endpoint(
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> axum::response::Response {
        const SESSION_ID: &str = "test-session-123";

        let method = body.get("method").and_then(Value::as_str).unwrap_or("");
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let has_session = headers
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            == Some(SESSION_ID);

        match method {
            "initialize" => (
                [("mcp-session-id", SESSION_ID)],
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "session-test", "version": "1.0.0" }
                    }
                })),
            )
                .into_response(),
            "notifications/initialized" if has_session => StatusCode::ACCEPTED.into_response(),
            "tools/list" if has_session => Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [{
                        "name": "resolve-library-id",
                        "description": "Resolve docs",
                        "inputSchema": { "type": "object" }
                    }]
                }
            }))
            .into_response(),
            _ => session_required_error_response(),
        }
    }

    async fn streamable_jsonrpc_error() -> impl IntoResponse {
        session_required_error_response()
    }

    fn session_required_error_response() -> axum::response::Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32000,
                    "message": "Bad Request: No valid session ID provided"
                },
                "id": null
            })),
        )
            .into_response()
    }

    #[test]
    fn sse_parser_skips_comments_and_accepts_crlf_delimiters() {
        let mut buffer =
            ": keep-alive\r\n\r\nevent: endpoint\r\ndata: /messages\r\n\r\n".to_string();

        let event = take_sse_event(&mut buffer).expect("endpoint event should parse");

        assert_eq!(event.event.as_deref(), Some("endpoint"));
        assert_eq!(event.data, "/messages");
        assert!(buffer.is_empty());
    }

    #[test]
    fn sse_endpoint_must_stay_on_the_configured_origin() {
        assert_eq!(
            resolve_sse_endpoint("http://127.0.0.1:3000/sse", "/messages")
                .expect("relative endpoint should resolve"),
            "http://127.0.0.1:3000/messages"
        );
        assert_eq!(
            resolve_sse_endpoint(
                "http://127.0.0.1:3000/sse",
                "http://127.0.0.1:3000/messages"
            )
            .expect("same-origin absolute endpoint should resolve"),
            "http://127.0.0.1:3000/messages"
        );
        assert_eq!(
            resolve_sse_endpoint(
                "http://127.0.0.1:3000/sse",
                "https://attacker.example/messages"
            )
            .expect_err("cross-origin endpoint should be rejected"),
            "SSE endpoint must be on the same origin as the configured URL"
        );
    }

    #[test]
    fn resolve_http_headers_prefers_server_env_then_process_env_and_drops_missing_values() {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_string(),
            "Bearer {env:MOOR_HEADER_TOKEN}".to_string(),
        );
        headers.insert(
            "X-Process".to_string(),
            "{env:MOOR_PROCESS_HEADER_TOKEN}".to_string(),
        );
        headers.insert(
            "X-Missing".to_string(),
            "{env:MOOR_MISSING_HEADER_TOKEN}".to_string(),
        );
        headers.insert("X-Static".to_string(), "static".to_string());

        let mut server_env = HashMap::new();
        server_env.insert("MOOR_HEADER_TOKEN".to_string(), "server-token".to_string());

        let resolved =
            resolve_http_headers_with_process_env(Some(&headers), Some(&server_env), |name| {
                match name {
                    "MOOR_HEADER_TOKEN" => Some("process-token".to_string()),
                    "MOOR_PROCESS_HEADER_TOKEN" => Some("process-fallback".to_string()),
                    _ => None,
                }
            });

        assert_eq!(
            resolved.get("Authorization").map(String::as_str),
            Some("Bearer server-token")
        );
        assert_eq!(
            resolved.get("X-Process").map(String::as_str),
            Some("process-fallback")
        );
        assert_eq!(resolved.get("X-Static").map(String::as_str), Some("static"));
        assert!(!resolved.contains_key("X-Missing"));
    }

    #[tokio::test]
    async fn independent_probe_timeout_preserves_tool_timeout_and_rpc_codes() {
        let app = Router::new().route("/mcp", post(|Json(body): Json<Value>| async move {
            if body["method"] == "ping" {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Json(serde_json::json!({ "jsonrpc": "2.0", "id": body["id"], "error": { "code": -32601, "message": "Method not found" } }))
            } else { Json(serde_json::json!({ "jsonrpc": "2.0", "id": body["id"], "result": {} })) }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let transport = HttpClientTransport::new(
            &format!("http://{addr}/mcp"),
            HashMap::new(),
            Duration::from_secs(30),
        );
        assert!(transport
            .send_request_with_timeout(1, "ping", None, Duration::from_millis(20))
            .await
            .unwrap_err()
            .message
            .contains("timed out"));
        assert_eq!(transport.request_timeout, Duration::from_secs(30));
        assert_eq!(
            transport
                .send_request_with_timeout(2, "ping", None, Duration::from_secs(1))
                .await
                .unwrap_err()
                .code,
            Some(-32601)
        );
        assert_eq!(
            transport
                .send_request(3, "tools/call", Some(serde_json::json!({})))
                .await
                .unwrap(),
            serde_json::json!({})
        );
        server.abort();
    }

    #[tokio::test]
    async fn request_falls_back_to_legacy_sse_transport() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let state = SseTestState {
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
        };
        let app = Router::new()
            .route("/sse", get(sse_endpoint).post(streamable_unsupported))
            .route("/messages", post(message_endpoint))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to read local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        let transport = HttpClientTransport::new(
            &format!("http://{addr}/sse"),
            HashMap::new(),
            Duration::from_secs(30),
        );
        let result = transport
            .send_request(1, "initialize", Some(serde_json::json!({})))
            .await
            .expect("SSE fallback request should succeed");

        assert_eq!(result["ok"], true);
        server.abort();
    }

    #[tokio::test]
    async fn streamable_http_reuses_session_id_after_initialize() {
        let app = Router::new().route("/mcp", post(streamable_session_endpoint));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to read local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        let client = McpClient::connect_http(HttpConnectConfig {
            server_name: "session-test".to_string(),
            url: format!("http://{addr}/mcp"),
            headers: HashMap::new(),
            request_timeout_ms: 30_000,
        })
        .await
        .expect("handshake should reuse session header for initialized notification");
        let tools = client
            .list_tools()
            .await
            .expect("tools/list should reuse session header");

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "resolve-library-id");
        server.abort();
    }

    #[tokio::test]
    async fn streamable_http_error_response_uses_jsonrpc_message() {
        let app = Router::new().route("/mcp", post(streamable_jsonrpc_error));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to read local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        let transport = HttpClientTransport::new(
            &format!("http://{addr}/mcp"),
            HashMap::new(),
            Duration::from_secs(30),
        );
        let err = transport
            .send_request(1, "tools/list", Some(serde_json::json!({})))
            .await
            .expect_err("JSON-RPC error body should be returned as a transport error");

        assert_eq!(
            err,
            "Remote MCP server error: Bad Request: No valid session ID provided"
        );
        server.abort();
    }

    #[tokio::test]
    async fn streamable_json_response_uses_configured_request_timeout() {
        let app = Router::new().route("/mcp", post(delayed_streamable_json));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to read local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        let transport = HttpClientTransport::new(
            &format!("http://{addr}/mcp"),
            HashMap::new(),
            Duration::from_millis(50),
        );
        let err = transport
            .send_request(1, "initialize", Some(serde_json::json!({})))
            .await
            .expect_err("slow JSON response should time out");

        assert!(err.contains("timed out"));
        server.abort();
    }

    #[tokio::test]
    async fn sse_fallback_request_uses_overall_request_timeout() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let state = SseTestState {
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
        };
        let app = Router::new()
            .route("/sse", get(sse_endpoint).post(streamable_unsupported))
            .route("/messages", post(delayed_progress_message_endpoint))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind test server");
        let addr = listener.local_addr().expect("failed to read local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        let transport = HttpClientTransport::new(
            &format!("http://{addr}/sse"),
            HashMap::new(),
            Duration::from_millis(50),
        );
        let err = transport
            .send_request(1, "initialize", Some(serde_json::json!({})))
            .await
            .expect_err("SSE fallback request should use the overall request timeout");

        assert!(err.contains("timed out after 50ms"));
        server.abort();
    }
}
