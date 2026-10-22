//! 工具检索：策展模式下 `moor_search_tools` 的匹配、分页与审计。
//!
//! 协议层只负责参数校验与响应编码（见 CONTEXT.md 的 Server Runtime），
//! 检索逻辑与它产生的 Audit Log 归属都留在这里。

use crate::sidecar::db::Database;
use crate::sidecar::services::audit_recorder::{AuditRecorder, ToolCallRecord, ToolCallVia};
use crate::sidecar::services::server_manager::{ServerManager, ToolCallError};
use crate::sidecar::services::tool_catalog::ToolCatalogEntry;
use serde_json::{json, Value};

/// 策展模式下注入的检索元工具。名字只有单个下划线，与 `{slug}__{tool}` 不冲突。
pub const SEARCH_TOOL_NAME: &str = "moor_search_tools";

const DEFAULT_LIMIT: usize = 10;
/// 元工具 schema 与运行时夹取上限共用的上限。
pub const MAX_SEARCH_LIMIT: usize = 50;

pub struct SearchOutcome {
    pub payload: Value,
}

/// 检索可调用工具，并记录一条来源为 `meta` 的审计。
///
/// Active Profile 只在这里读一次：检索池与审计行使用同一个归属，
/// 调用途中切 Profile 不会把这次检索记到新 Profile 上。
pub async fn search(
    manager: &ServerManager,
    db: &Database,
    params: &Value,
    agent_info: Option<&str>,
) -> Result<SearchOutcome, ToolCallError> {
    let started = std::time::Instant::now();
    let exposure = manager.tool_exposure().await;
    let profile_id = exposure
        .as_ref()
        .ok()
        .and_then(|exposure| exposure.profile_id.as_deref());
    let result = exposure
        .as_ref()
        .ok()
        .map(|exposure| search_catalog(&exposure.callable_tools(), params));
    let recorded_result = result.as_ref().map(|(_, hits)| json!({ "hits": hits }));
    let error = exposure.as_ref().err().map(ToString::to_string);
    AuditRecorder::record(
        db,
        ToolCallRecord {
            profile_id,
            server_id: None,
            tool_name: SEARCH_TOOL_NAME,
            arguments: params,
            result: recorded_result.as_ref(),
            error: error.as_deref(),
            duration_ms: started.elapsed().as_millis() as i64,
            agent_info,
            via: ToolCallVia::Meta,
            refusal: None,
        },
    );
    exposure?;
    Ok(SearchOutcome {
        payload: result.expect("successful exposure has search results").0,
    })
}

/// 关键词按空格分隔且全部命中（大小写不敏感），在工具名与描述上匹配。
/// `cursor` 是上一页返回的 offset，`limit` 被夹在 1..=50。
fn search_catalog(catalog: &[ToolCatalogEntry], params: &Value) -> (Value, usize) {
    let query = params.get("query").and_then(Value::as_str).unwrap_or("");
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    let server_filter = params
        .get("server")
        .and_then(Value::as_str)
        .map(|server| server.to_lowercase());
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_LIMIT as u64)
        .clamp(1, MAX_SEARCH_LIMIT as u64) as usize;
    let offset = params
        .get("cursor")
        .and_then(Value::as_str)
        .and_then(|cursor| cursor.parse::<usize>().ok())
        .unwrap_or(0);

    let matched: Vec<&ToolCatalogEntry> = catalog
        .iter()
        .filter(|entry| {
            if let Some(server) = &server_filter {
                if !entry.server_name.to_lowercase().contains(server) {
                    return false;
                }
            }
            if terms.is_empty() {
                return true;
            }
            let haystack = format!(
                "{} {} {}",
                entry.tool_name,
                entry.exposed_name,
                entry.description.clone().unwrap_or_default()
            )
            .to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .collect();
    let hits = matched.len();

    let tools: Vec<Value> = matched
        .iter()
        .skip(offset)
        .take(limit)
        .map(|entry| {
            json!({
                "name": entry.exposed_name,
                "description": entry.description,
                "inputSchema": entry
                    .input_schema
                    .clone()
                    .unwrap_or_else(|| json!({"type": "object"})),
                "serverName": entry.server_name,
                "riskLevel": entry.risk_level,
            })
        })
        .collect();

    let mut payload = json!({ "tools": tools });
    if hits > offset + limit {
        payload["nextCursor"] = json!((offset + limit).to_string());
    }
    (payload, hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tool_name: &str, description: Option<&str>) -> ToolCatalogEntry {
        ToolCatalogEntry {
            server_name: "Alpha".to_string(),
            risk_level: "unknown".to_string(),
            tool_name: tool_name.to_string(),
            exposed_name: format!("alpha__{tool_name}"),
            description: description.map(|text| text.to_string()),
            input_schema: None,
        }
    }

    #[test]
    fn every_keyword_must_match() {
        let catalog = vec![
            entry("search_issues", Some("Find issues in a repository")),
            entry("delete_repo", None),
        ];

        let (payload, hits) = search_catalog(&catalog, &json!({ "query": "issues repo" }));
        assert_eq!(hits, 1);
        assert_eq!(payload["tools"][0]["name"], "alpha__search_issues");
    }

    #[test]
    fn caps_the_limit_and_offers_a_cursor() {
        let catalog: Vec<_> = (0..5)
            .map(|index| entry(&format!("tool{index}"), None))
            .collect();

        let (first, hits) = search_catalog(&catalog, &json!({ "limit": 2 }));
        assert_eq!(hits, 5);
        assert_eq!(first["tools"].as_array().expect("tools").len(), 2);
        assert_eq!(first["nextCursor"], "2");

        let cursor = first["nextCursor"].clone();
        let (second, _) = search_catalog(&catalog, &json!({ "limit": 2, "cursor": cursor }));
        assert_eq!(second["tools"][0]["name"], "alpha__tool2");
    }

    #[test]
    fn filters_by_server_name() {
        let catalog = vec![
            entry("search", None),
            ToolCatalogEntry {
                server_name: "Beta".to_string(),
                ..entry("search-beta", None)
            },
        ];

        let (payload, hits) = search_catalog(&catalog, &json!({ "server": "beta" }));
        assert_eq!(hits, 1);
        assert_eq!(payload["tools"][0]["serverName"], "Beta");
    }
}
