use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RequestError {
    pub code: Option<i64>,
    pub message: String,
}

impl From<String> for RequestError {
    fn from(message: String) -> Self {
        Self {
            code: None,
            message,
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
            code: error.get("code").and_then(Value::as_i64),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown MCP error")
                .to_string(),
        }
    }
}

pub fn parse_response(body: &Value) -> Result<Value, RequestError> {
    if let Some(error) = body.get("error") {
        return Err(RequestError::from_rpc(error));
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}
