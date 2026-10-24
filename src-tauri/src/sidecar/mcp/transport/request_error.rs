use serde_json::Value;

/// 错误来自哪一层。用来区分「下游明确报错」与「本地传输/超时」，
/// 避免靠错误文案猜分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestErrorKind {
    /// 下游返回了 JSON-RPC error。
    Rpc,
    /// 请求超时。
    Timeout,
    /// 传输层或其他本地失败。
    Transport,
}

#[derive(Debug, Clone)]
pub struct RequestError {
    pub kind: RequestErrorKind,
    pub http_status: Option<u16>,
    pub code: Option<i64>,
    pub message: String,
    /// 下游 `error.data`，原样保留给上游。
    pub data: Option<Value>,
}

impl From<String> for RequestError {
    fn from(message: String) -> Self {
        Self {
            kind: RequestErrorKind::Transport,
            http_status: None,
            code: None,
            message,
            data: None,
        }
    }
}

impl RequestError {
    pub fn timeout(message: String) -> Self {
        Self {
            kind: RequestErrorKind::Timeout,
            http_status: None,
            code: None,
            message,
            data: None,
        }
    }
}

impl From<&str> for RequestError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl RequestError {
    pub fn from_rpc(error: &Value) -> Self {
        Self {
            kind: RequestErrorKind::Rpc,
            http_status: None,
            code: error.get("code").and_then(Value::as_i64),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown MCP error")
                .to_string(),
            data: error.get("data").cloned(),
        }
    }
}

impl RequestError {
    pub fn is_protocol_version_error(&self) -> bool {
        if !matches!(
            (self.kind, self.code, self.http_status),
            (RequestErrorKind::Rpc, Some(-32602), _) | (RequestErrorKind::Transport, _, Some(400))
        ) {
            return false;
        }
        let message = self.message.to_ascii_lowercase();
        message.contains("unsupported protocol version")
            || message.contains("invalid protocol version")
            || message.contains("protocol version is not supported")
    }

    pub fn with_fallback_error(self, error: &RequestError) -> Self {
        let excerpt = |error: &RequestError| {
            let diagnostic = error.diagnostic_message();
            let mut chars = diagnostic.chars();
            let mut excerpt: String = chars.by_ref().take(1024).collect();
            if chars.next().is_some() {
                excerpt.push_str(" [truncated]");
            }
            excerpt
        };
        Self {
            message: format!(
                "Legacy SSE failed: {}; initial Streamable HTTP failure: {}",
                excerpt(error),
                excerpt(&self)
            ),
            ..error.clone()
        }
    }

    pub fn diagnostic_message(&self) -> String {
        let mut message = self.message.clone();
        if let Some(status) = self.http_status {
            message = format!("HTTP {status}: {message}");
        }
        if let Some(code) = self.code {
            message.push_str(&format!(" (RPC {code})"));
        }
        if let Some(data) = &self.data {
            let data = crate::sidecar::services::audit_redaction::redact_for_audit(data);
            message.push_str(&format!(" data={data}"));
        }
        let redacted =
            crate::sidecar::services::server_log::redact_sensitive_stderr_values(&message);
        let mut bounded: String = redacted.chars().take(4096).collect();
        if bounded.len() < redacted.len() {
            bounded.push_str(" [truncated]");
        }
        bounded
    }
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.diagnostic_message())
    }
}

pub fn parse_response(body: &Value) -> Result<Value, RequestError> {
    if let Some(error) = body.get("error") {
        return Err(RequestError::from_rpc(error));
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}
