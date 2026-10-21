use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

// JSON-RPC 2.0 error codes
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

// Moor 侧错误码：让 Agent 能区分「超时 / 被策略拦 / 服务没起」。
pub const UPSTREAM_TIMEOUT: i64 = -32001;
pub const POLICY_DENIED: i64 = -32002;
pub const SERVER_UNAVAILABLE: i64 = -32003;

pub fn make_response(id: Id, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub fn make_error(id: Id, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// 带 `data` 的错误：策略拒绝需要把风险等级与建议一起透给 Agent。
pub fn make_error_with_data(id: Id, code: i64, message: &str, data: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message, "data": data }
    })
}

pub fn parse_request_value(v: &Value) -> Option<(Id, String, Option<Value>)> {
    if v.get("jsonrpc")?.as_str()? != "2.0" {
        return None;
    }
    let id = v.get("id")?;
    let method = v.get("method")?.as_str()?.to_string();
    let params = v.get("params").cloned();
    let id = match id {
        Value::Number(n) => Id::Number(n.as_i64()?),
        Value::String(s) => Id::String(s.clone()),
        _ => return None,
    };
    Some((id, method, params))
}
