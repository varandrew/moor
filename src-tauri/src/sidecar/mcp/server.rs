use super::jsonrpc;
use crate::sidecar::http::AppState;
use crate::sidecar::services::audit_recorder::ToolCallVia;
use crate::sidecar::services::server_manager::ToolCallError;
use crate::sidecar::services::tool_search::MAX_SEARCH_LIMIT;
use crate::sidecar::services::tool_visibility::ExposureMode;
use serde_json::{json, Value};
use std::sync::Arc;

pub use crate::sidecar::services::tool_search::SEARCH_TOOL_NAME;
/// 策展模式下注入的转发元工具。
pub const CALL_TOOL_NAME: &str = "moor_call_tool";

/// Handle an incoming MCP JSON-RPC request and produce a response.
/// This implements the MCP Gateway: aggregates tools from all running servers
/// and routes tool calls to the correct server.
pub async fn handle_request(
    id: jsonrpc::Id,
    method: &str,
    params: Option<serde_json::Value>,
    state: Arc<AppState>,
    agent_info: Option<&str>,
) -> serde_json::Value {
    match method {
        "initialize" => handle_initialize(id),
        "tools/list" => handle_tools_list(id, state).await,
        "tools/call" => handle_tools_call(id, params, state, agent_info).await,
        "ping" => jsonrpc::make_response(id, serde_json::json!({})),
        _ => jsonrpc::make_error(
            id,
            jsonrpc::METHOD_NOT_FOUND,
            &format!("Unknown method: {method}"),
        ),
    }
}

fn handle_initialize(id: jsonrpc::Id) -> serde_json::Value {
    jsonrpc::make_response(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": { "name": "Moor", "version": env!("CARGO_PKG_VERSION") },
        }),
    )
}

async fn handle_tools_list(id: jsonrpc::Id, state: Arc<AppState>) -> serde_json::Value {
    let exposure = match state.server_manager.tool_exposure().await {
        Ok(exposure) => exposure,
        Err(error) => return jsonrpc::make_error(id, jsonrpc::INTERNAL_ERROR, &error.to_string()),
    };
    let catalog = exposure.listed_tools();
    let mut tools: Vec<serde_json::Value> = catalog
        .iter()
        .map(|tool| {
            json!({
                "name": tool.exposed_name,
                "description": tool.description,
                "inputSchema": tool
                    .input_schema
                    .clone()
                    .unwrap_or_else(|| json!({"type": "object"})),
                "_meta": { "serverName": tool.server_name },
            })
        })
        .collect();
    if matches!(exposure.mode, ExposureMode::Curated) {
        tools.extend(meta_tool_definitions());
    }
    jsonrpc::make_response(id, json!({ "tools": tools }))
}

/// 策展模式的两个元工具。`moor_search_tools` 的描述是产品文案——它决定模型会不会先搜索。
fn meta_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": SEARCH_TOOL_NAME,
            "description": "Search the tools this gateway can expose but is not listing right now. Call this before claiming a tool is unavailable: when the tool you need is not in the current list, search first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Space-separated keywords; all must match" },
                    "server": { "type": "string", "description": "Only search tools from servers whose name contains this" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_LIMIT },
                    "cursor": { "type": "string", "description": "Opaque cursor from a previous search" }
                }
            },
            "_meta": { "provider": "moor" }
        }),
        json!({
            "name": CALL_TOOL_NAME,
            "description": "Call a tool by the name returned from moor_search_tools. The result has the same shape as tools/call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "arguments": { "type": "object" }
                },
                "required": ["name"]
            },
            "_meta": { "provider": "moor" }
        }),
    ]
}

async fn handle_tools_call(
    id: jsonrpc::Id,
    params: Option<serde_json::Value>,
    state: Arc<AppState>,
    agent_info: Option<&str>,
) -> serde_json::Value {
    let params = match params {
        Some(p) => p,
        None => return jsonrpc::make_error(id, jsonrpc::INVALID_PARAMS, "Missing params"),
    };
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return jsonrpc::make_error(id, jsonrpc::INVALID_PARAMS, "Missing tool name"),
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if tool_name == SEARCH_TOOL_NAME {
        let outcome = match crate::sidecar::services::tool_search::search(
            &state.server_manager,
            &state.db,
            &arguments,
            agent_info,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return jsonrpc::make_error(id, jsonrpc::INTERNAL_ERROR, &error.to_string())
            }
        };
        return jsonrpc::make_response(
            id,
            json!({
                "content": [{ "type": "text", "text": outcome.payload.to_string() }],
            }),
        );
    }

    if tool_name == CALL_TOOL_NAME {
        let Some(inner_name) = arguments.get("name").and_then(Value::as_str) else {
            return jsonrpc::make_error(
                id,
                jsonrpc::INVALID_PARAMS,
                &format!("{CALL_TOOL_NAME} requires a tool name"),
            );
        };
        let inner_arguments = arguments
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        return call_and_map(
            id,
            state,
            inner_name,
            inner_arguments,
            agent_info,
            ToolCallVia::Meta,
        )
        .await;
    }

    call_and_map(
        id,
        state,
        tool_name,
        arguments,
        agent_info,
        ToolCallVia::Direct,
    )
    .await
}

async fn call_and_map(
    id: jsonrpc::Id,
    state: Arc<AppState>,
    tool_name: &str,
    arguments: Value,
    agent_info: Option<&str>,
    via: ToolCallVia,
) -> Value {
    match state
        .server_manager
        .call_tool(tool_name, arguments, agent_info, via)
        .await
    {
        Ok(result) => jsonrpc::make_response(id, result),
        Err(ToolCallError::InvalidTool(err)) => {
            jsonrpc::make_error(id, jsonrpc::INVALID_PARAMS, &err)
        }
        Err(ToolCallError::PolicyDenied { risk_level, hint }) => jsonrpc::make_error_with_data(
            id,
            jsonrpc::POLICY_DENIED,
            "Tool blocked by risk policy",
            json!({ "policy": "risk", "riskLevel": risk_level, "hint": hint }),
        ),
        Err(ToolCallError::ServerUnavailable(err)) => {
            jsonrpc::make_error(id, jsonrpc::SERVER_UNAVAILABLE, &err)
        }
        Err(ToolCallError::Timeout(err)) => {
            jsonrpc::make_error(id, jsonrpc::UPSTREAM_TIMEOUT, &err)
        }
        Err(ToolCallError::Downstream {
            code,
            message,
            data,
        }) => match data {
            Some(data) => jsonrpc::make_error_with_data(id, code, &message, data),
            None => jsonrpc::make_error(id, code, &message),
        },
        Err(ToolCallError::Execution(err)) => {
            jsonrpc::make_error(id, jsonrpc::INTERNAL_ERROR, &err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::audit_log_repo::AuditLogRepository;
    use crate::sidecar::db::profile_repo::{ProfileRepository, ProfileServerUpsertInput};
    use crate::sidecar::db::server_repo::ServerRepository;
    use crate::sidecar::db::Database;
    use crate::sidecar::services::event_bus::EventBus;
    use crate::sidecar::services::server_manager::ServerManager;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-mcp-{test_name}-{timestamp}"))
    }

    fn write_fake_mcp_server_with_tools(
        data_dir: &std::path::Path,
        tools: &str,
    ) -> std::path::PathBuf {
        let script = data_dir.join("fake-mcp-server.mjs");
        let template = r#"
let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  const lines = buffer.split("\n");
  buffer = lines.pop() ?? "";
  for (const line of lines) {
    if (!line.trim()) continue;
    const request = JSON.parse(line);
    if (request.id === undefined) continue;
    let result = {};
    if (request.method === "initialize") {
      result = { protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "fake", version: "1.0.0" } };
    } else if (request.method === "tools/list") {
      const tools = __TOOLS__;
      result = { tools: tools.map((tool) => ({ name: tool.name, description: "Fake tool", inputSchema: { type: "object" } })) };
    } else if (request.method === "tools/call") {
      if (request.params.name === "fail_with_data") {
        process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, error: { code: -32010, message: "boom", data: { hint: "check input" } } }) + "\n");
        continue;
      }
      result = { content: [{ type: "text", text: JSON.stringify(request.params.arguments ?? {}) }] };
    }
    process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
  }
});
"#;
        std::fs::write(&script, template.replace("__TOOLS__", tools))
            .expect("failed to write fake MCP server");
        script
    }

    struct FakeGateway {
        data_dir: std::path::PathBuf,
        db: Arc<Database>,
        state: Arc<AppState>,
        server_id: String,
    }

    impl Drop for FakeGateway {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.data_dir);
        }
    }

    async fn start_test_gateway(test_name: &str) -> FakeGateway {
        start_test_gateway_with_tools(test_name, r#"[{ "name": "echo" }]"#).await
    }

    async fn start_test_gateway_with_tools(test_name: &str, tools: &str) -> FakeGateway {
        let data_dir = temp_data_dir(test_name);
        std::fs::create_dir_all(&data_dir).expect("failed to create temp data dir");
        let script = write_fake_mcp_server_with_tools(&data_dir, tools);

        let db = Arc::new(Database::open(&data_dir.join("moor.db")).expect("failed to open db"));
        db.run_migrations().expect("failed to migrate");
        crate::sidecar::services::settings::init_settings(db.as_ref(), &data_dir)
            .expect("failed to initialize settings");
        let profile_repo = ProfileRepository::new(&db);
        profile_repo.seed_default().expect("failed to seed profile");

        let server_id = uuid::Uuid::new_v4().to_string();
        ServerRepository::new(&db)
            .insert_one_with_id(
                &server_id,
                0,
                &crate::sidecar::db::server_repo::ServerInsertInput {
                    name: "fake".into(),
                    connection_type: "stdio".into(),
                    command: Some("node".into()),
                    args: Some(vec![script.to_string_lossy().to_string()]),
                    url: None,
                    env: None,
                    headers: None,
                    working_dir: None,
                    auto_start: false,
                },
            )
            .expect("failed to insert server");
        profile_repo
            .assign_to_active_profile(std::slice::from_ref(&server_id))
            .expect("failed to assign server");

        let event_bus = Arc::new(EventBus::new(16));
        let server_manager = Arc::new(ServerManager::new(db.clone(), event_bus.clone()));
        server_manager.load_from_db().await;
        server_manager
            .start_server(&server_id)
            .await
            .expect("failed to start fake server");

        let state = Arc::new(AppState {
            db: db.clone(),
            api_token: "test-token".to_string(),
            version: "test".to_string(),
            port: 19323,
            mcp_sessions: Arc::new(
                crate::sidecar::mcp::transport::mcp_session::McpSessionStore::new(),
            ),
            event_bus,
            server_manager,
        });

        FakeGateway {
            data_dir,
            db,
            state,
            server_id,
        }
    }

    #[tokio::test]
    async fn tools_call_routes_to_running_server_and_records_audit() {
        let gateway = start_test_gateway("tools-call").await;

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            tools_response["result"]["tools"][0]["_meta"]["serverName"],
            "fake"
        );
        assert!(tools_response["result"]["tools"][0]
            .get("annotations")
            .is_none());

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({
                "name": "fake__echo",
                "arguments": { "token": "secret", "value": "ok" }
            })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;

        assert_eq!(response["result"]["content"][0]["type"], "text");
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"value\":\"ok\""));

        let audit_repo = AuditLogRepository::new(&gateway.db);
        let logs = audit_repo
            .query_logs(None, Some("fake__echo"), None, None, Some(10), None)
            .expect("failed to query audit logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].server_id.as_deref(),
            Some(gateway.server_id.as_str())
        );
        assert_eq!(logs[0].arguments.as_ref().unwrap()["token"], "[REDACTED]");
    }

    #[tokio::test]
    async fn disabling_a_tool_removes_it_from_list_and_rejects_the_call() {
        let gateway = start_test_gateway("disabled-tool").await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .upsert_profile_server(
                &profile_id,
                &gateway.server_id,
                None,
                Some(&vec!["echo".to_string()]),
            )
            .expect("failed to disable tool");

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools should be an array")
                .len(),
            0,
            "a denied tool must not be listed"
        );

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__echo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(response["error"]["code"], jsonrpc::INVALID_PARAMS);
        assert_eq!(
            response["error"]["message"], "Tool \"fake__echo\" not found or disabled",
            "the deny-list error must stay byte-identical"
        );
    }

    #[tokio::test]
    async fn disabling_a_server_removes_its_tools_and_rejects_calls() {
        let gateway = start_test_gateway("disabled-server").await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .upsert_profile_server(&profile_id, &gateway.server_id, Some(false), None)
            .expect("failed to disable server");

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools should be an array")
                .len(),
            0,
            "a disabled server must not contribute tools"
        );

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__echo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(response["error"]["code"], jsonrpc::INVALID_PARAMS);
        assert_eq!(
            response["error"]["message"],
            "Tool \"fake__echo\" not found or disabled"
        );
    }

    #[tokio::test]
    async fn curated_profile_lists_only_pinned_tools_but_calls_unpinned_ones() {
        let gateway = start_test_gateway_with_tools(
            "curated",
            r#"[{ "name": "echo" }, { "name": "report" }]"#,
        )
        .await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .update(
                &profile_id,
                crate::sidecar::db::profile_repo::ProfileUpdate {
                    exposure_mode: Some("curated"),
                    ..Default::default()
                },
            )
            .expect("failed to switch exposure mode");
        profile_repo
            .upsert_profile_servers_batch(
                &profile_id,
                &[ProfileServerUpsertInput {
                    server_id: gateway.server_id.clone(),
                    enabled: None,
                    disabled_tools: None,
                    pinned_tools: Some(vec!["echo".to_string()]),
                }],
            )
            .expect("failed to pin a tool");

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        let listed = tools_response["result"]["tools"]
            .as_array()
            .expect("tools should be an array");
        let names: Vec<&str> = listed
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(
            names.contains(&"fake__echo"),
            "curated mode lists the pinned tool"
        );
        assert!(
            !names.contains(&"fake__report"),
            "curated mode hides unpinned tools"
        );
        assert!(names.contains(&SEARCH_TOOL_NAME));
        assert!(names.contains(&CALL_TOOL_NAME));

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__report", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            response["result"]["content"][0]["type"], "text",
            "an unpinned tool stays callable so cached names keep working"
        );
    }

    #[tokio::test]
    async fn direct_profile_lists_unpinned_tools_too() {
        let gateway = start_test_gateway_with_tools(
            "direct-pinned",
            r#"[{ "name": "echo" }, { "name": "report" }]"#,
        )
        .await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .upsert_profile_servers_batch(
                &profile_id,
                &[ProfileServerUpsertInput {
                    server_id: gateway.server_id.clone(),
                    enabled: None,
                    disabled_tools: None,
                    pinned_tools: Some(vec!["echo".to_string()]),
                }],
            )
            .expect("failed to pin a tool");

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        let listed = tools_response["result"]["tools"]
            .as_array()
            .expect("tools should be an array");
        assert_eq!(listed.len(), 2, "direct mode ignores pinning");
    }

    #[tokio::test]
    async fn direct_profile_exposes_no_meta_tools() {
        let gateway = start_test_gateway("no-meta").await;

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        let names: Vec<&str> = tools_response["result"]["tools"]
            .as_array()
            .expect("tools should be an array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(!names.contains(&SEARCH_TOOL_NAME));
        assert!(!names.contains(&CALL_TOOL_NAME));
    }

    async fn curated_gateway(test_name: &str) -> FakeGateway {
        let gateway = start_test_gateway_with_tools(
            test_name,
            r#"[{ "name": "echo" }, { "name": "report" }]"#,
        )
        .await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .update(
                &profile_id,
                crate::sidecar::db::profile_repo::ProfileUpdate {
                    exposure_mode: Some("curated"),
                    ..Default::default()
                },
            )
            .expect("failed to switch exposure mode");
        gateway
    }

    async fn search(state: Arc<AppState>, arguments: serde_json::Value) -> serde_json::Value {
        let response = handle_request(
            jsonrpc::Id::Number(2),
            "tools/call",
            Some(serde_json::json!({ "name": SEARCH_TOOL_NAME, "arguments": arguments })),
            state,
            Some("test-agent"),
        )
        .await;
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("search should answer with a text payload");
        serde_json::from_str(text).expect("search payload should be json")
    }

    #[tokio::test]
    async fn search_tools_finds_unpinned_tools_by_keyword() {
        let gateway = curated_gateway("search-keyword").await;

        let result = search(
            gateway.state.clone(),
            serde_json::json!({ "query": "report" }),
        )
        .await;
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "fake__report");
        assert_eq!(tools[0]["serverName"], "fake");
        assert_eq!(tools[0]["inputSchema"]["type"], "object");
        assert_eq!(
            tools[0]["riskLevel"], "unknown",
            "search results must carry the risk level"
        );
    }

    #[tokio::test]
    async fn search_tools_paginates_with_a_cursor() {
        let gateway = curated_gateway("search-cursor").await;

        let first = search(gateway.state.clone(), serde_json::json!({ "limit": 1 })).await;
        assert_eq!(
            first["tools"].as_array().expect("tools array").len(),
            1,
            "limit must cap the page"
        );
        let cursor = first["nextCursor"]
            .as_str()
            .expect("a partial page must offer a cursor");

        let second = search(
            gateway.state.clone(),
            serde_json::json!({ "limit": 1, "cursor": cursor }),
        )
        .await;
        assert_eq!(second["tools"].as_array().expect("tools array").len(), 1);
        assert_ne!(first["tools"][0]["name"], second["tools"][0]["name"]);
    }

    #[tokio::test]
    async fn call_tool_forwards_to_the_named_tool() {
        let gateway = curated_gateway("meta-call").await;

        let response = handle_request(
            jsonrpc::Id::Number(3),
            "tools/call",
            Some(serde_json::json!({
                "name": CALL_TOOL_NAME,
                "arguments": { "name": "fake__report", "arguments": { "value": "ok" } }
            })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;

        assert_eq!(response["result"]["content"][0]["type"], "text");
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .expect("text payload")
            .contains("\"value\":\"ok\""));
    }

    #[tokio::test]
    async fn blocking_policy_hides_and_rejects_destructive_tools() {
        let gateway =
            start_test_gateway_with_tools("risk-blocked", r#"[{ "name": "delete_repo" }]"#).await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to read active profile")
            .expect("active profile should exist");
        profile_repo
            .update(
                &profile_id,
                crate::sidecar::db::profile_repo::ProfileUpdate {
                    risk_policy: Some("block_destructive"),
                    ..Default::default()
                },
            )
            .expect("failed to set the risk policy");

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools should be an array")
                .len(),
            0,
            "a blocked tool must not be listed"
        );

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__delete_repo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(response["error"]["code"], jsonrpc::POLICY_DENIED);
        assert_eq!(response["error"]["data"]["policy"], "risk");
        assert_eq!(response["error"]["data"]["riskLevel"], "destructive");
        assert!(response["error"]["data"]["hint"].is_string());
    }

    #[tokio::test]
    async fn advisory_policy_still_lists_destructive_tools() {
        let gateway =
            start_test_gateway_with_tools("risk-advisory", r#"[{ "name": "delete_repo" }]"#).await;

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools should be an array")
                .len(),
            1
        );

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__delete_repo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        assert!(response.get("result").is_some(), "advisory mode only marks");
    }

    #[tokio::test]
    async fn a_stopped_server_answers_with_server_unavailable() {
        let gateway = start_test_gateway("server-unavailable").await;
        gateway
            .state
            .server_manager
            .stop_server(&gateway.server_id)
            .await
            .expect("failed to stop the fake server");

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__echo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;

        assert_eq!(response["error"]["code"], jsonrpc::SERVER_UNAVAILABLE);
    }

    #[tokio::test]
    async fn audit_distinguishes_direct_meta_and_search_calls() {
        let gateway = curated_gateway("audit-via").await;

        handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__report", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        handle_request(
            jsonrpc::Id::Number(2),
            "tools/call",
            Some(serde_json::json!({
                "name": CALL_TOOL_NAME,
                "arguments": { "name": "fake__echo", "arguments": {} }
            })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        search(
            gateway.state.clone(),
            serde_json::json!({ "query": "report" }),
        )
        .await;

        let repo = AuditLogRepository::new(&gateway.db);
        let rows = repo
            .query_logs(None, None, None, None, Some(20), None)
            .expect("audit rows");
        let via_of = |tool: &str| {
            rows.iter()
                .find(|row| row.tool_name == tool)
                .map(|row| row.via.clone())
        };
        assert_eq!(via_of("fake__report").as_deref(), Some("direct"));
        assert_eq!(via_of("fake__echo").as_deref(), Some("meta"));
        assert_eq!(via_of(SEARCH_TOOL_NAME).as_deref(), Some("meta"));

        let search_row = rows
            .iter()
            .find(|row| row.tool_name == SEARCH_TOOL_NAME)
            .expect("search row");
        assert_eq!(
            search_row
                .result
                .as_ref()
                .and_then(|value| value["hits"].as_i64()),
            Some(1),
            "a search records how many tools matched, not the payload"
        );
    }

    #[tokio::test]
    async fn a_policy_denial_is_audited() {
        let gateway =
            start_test_gateway_with_tools("audit-blocked", r#"[{ "name": "delete_repo" }]"#).await;
        let profile_repo = ProfileRepository::new(&gateway.db);
        let profile_id = profile_repo
            .find_active_id()
            .expect("read active profile")
            .expect("active profile");
        profile_repo
            .update(
                &profile_id,
                crate::sidecar::db::profile_repo::ProfileUpdate {
                    risk_policy: Some("block_destructive"),
                    ..Default::default()
                },
            )
            .expect("set policy");

        handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__delete_repo", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;

        let repo = AuditLogRepository::new(&gateway.db);
        let rows = repo
            .query_logs(None, Some("fake__delete_repo"), None, None, Some(10), None)
            .expect("audit rows");
        assert_eq!(rows.len(), 1, "a refused call still leaves a record");
        assert!(rows[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("blocked by risk policy")));
        assert_eq!(repo.get_insights(None, None).unwrap().blocked_calls, 1);
    }

    #[tokio::test]
    async fn a_curated_profile_with_no_pins_only_lists_the_meta_tools() {
        let gateway = curated_gateway("curated-empty").await;

        let tools_response = handle_request(
            jsonrpc::Id::Number(0),
            "tools/list",
            None,
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;
        let names: Vec<&str> = tools_response["result"]["tools"]
            .as_array()
            .expect("tools should be an array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();

        assert_eq!(names.len(), 2, "the empty profile still explains itself");
        assert!(names.contains(&SEARCH_TOOL_NAME));
        assert!(names.contains(&CALL_TOOL_NAME));
    }

    #[tokio::test]
    async fn a_downstream_error_keeps_its_code_and_data() {
        let gateway =
            start_test_gateway_with_tools("downstream-error", r#"[{ "name": "fail_with_data" }]"#)
                .await;

        let response = handle_request(
            jsonrpc::Id::Number(1),
            "tools/call",
            Some(serde_json::json!({ "name": "fake__fail_with_data", "arguments": {} })),
            gateway.state.clone(),
            Some("test-agent"),
        )
        .await;

        assert_eq!(response["error"]["code"], -32010);
        assert_eq!(response["error"]["message"], "boom");
        assert_eq!(response["error"]["data"]["hint"], "check input");
    }
}
