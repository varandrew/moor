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
    pub code: Option<i64>,
    pub message: String,
    /// 下游 `error.data`，原样保留给上游。
    pub data: Option<Value>,
}

impl From<String> for RequestError {
    fn from(message: String) -> Self {
        Self {
            kind: RequestErrorKind::Transport,
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

pub fn parse_response(body: &Value) -> Result<Value, RequestError> {
    if let Some(error) = body.get("error") {
        return Err(RequestError::from_rpc(error));
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}
