use crate::sidecar::db::tool_discovery_repo::ToolInsert;
use crate::sidecar::mcp::transport::http_client::HttpClientTransport;
use crate::sidecar::mcp::transport::stdio_client::StdioClientTransport;
use crate::sidecar::services::server_log::DiagnosticAttempt;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

pub struct McpClient {
    transport: McpTransport,
    server_name: String,
}

enum McpTransport {
    Stdio(StdioClientTransport),
    Http(HttpClientTransport),
}

pub struct StdioConnectConfig {
    pub server_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
    pub request_timeout_ms: u32,
    pub diagnostics: DiagnosticAttempt,
}

pub struct HttpConnectConfig {
    pub server_name: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub request_timeout_ms: u32,
}

impl McpClient {
    pub async fn connect_stdio(config: StdioConnectConfig) -> Result<Self, String> {
        let transport = StdioClientTransport::spawn(
            &config.command,
            &config.args,
            config.cwd.as_deref(),
            config.env,
            Duration::from_millis(config.request_timeout_ms as u64),
            config.diagnostics,
        )
        .await?;
        let mut client = Self {
            transport: McpTransport::Stdio(transport),
            server_name: config.server_name,
        };
        client.handshake().await?;
        Ok(client)
    }

    pub async fn connect_http(config: HttpConnectConfig) -> Result<Self, String> {
        let transport = HttpClientTransport::new(
            &config.url,
            config.headers,
            Duration::from_millis(config.request_timeout_ms as u64),
        );
        let mut client = Self {
            transport: McpTransport::Http(transport),
            server_name: config.server_name,
        };
        client.handshake().await?;
        Ok(client)
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInsert>, String> {
        let result = match &self.transport {
            McpTransport::Stdio(t) => t
                .send_request("tools/list", Some(serde_json::json!({})))
                .await
                .map_err(|err| self.enrich_stdio_error(&err))?,
            McpTransport::Http(t) => {
                let id = chrono::Utc::now().timestamp_millis();
                t.send_request(id, "tools/list", Some(serde_json::json!({})))
                    .await?
            }
        };
        Ok(parse_tools_list(&result))
    }

    pub async fn call_tool(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": args,
        });
        match &self.transport {
            McpTransport::Stdio(t) => t
                .send_request("tools/call", Some(params))
                .await
                .map_err(|err| self.enrich_stdio_error(&err)),
            McpTransport::Http(t) => {
                let id = chrono::Utc::now().timestamp_millis();
                t.send_request(id, "tools/call", Some(params)).await
            }
        }
    }

    pub async fn disconnect(&mut self) -> Result<(), String> {
        match &mut self.transport {
            McpTransport::Stdio(t) => t.close().await,
            McpTransport::Http(_) => Ok(()),
        }
    }

    pub fn set_request_timeout_ms(&mut self, request_timeout_ms: u32) {
        let request_timeout = Duration::from_millis(request_timeout_ms as u64);
        match &mut self.transport {
            McpTransport::Stdio(t) => t.set_request_timeout(request_timeout),
            McpTransport::Http(t) => t.set_request_timeout(request_timeout),
        }
    }

    pub fn alive_receiver(&self) -> Option<tokio::sync::watch::Receiver<bool>> {
        match &self.transport {
            McpTransport::Stdio(t) => Some(t.alive_receiver()),
            McpTransport::Http(_) => None,
        }
    }

    async fn handshake(&mut self) -> Result<(), String> {
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": format!("moor-{}", self.server_name),
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        match &mut self.transport {
            McpTransport::Stdio(t) => {
                let _ = t
                    .send_request("initialize", Some(init_params))
                    .await
                    .map_err(|err| t.with_startup_stderr_summary(err))?;
                t.send_notification("notifications/initialized", Some(serde_json::json!({})))
                    .await
                    .map_err(|err| t.with_startup_stderr_summary(err))?;
            }
            McpTransport::Http(t) => {
                let id = chrono::Utc::now().timestamp_millis();
                let _ = t.send_request(id, "initialize", Some(init_params)).await?;
                t.send_notification("notifications/initialized", Some(serde_json::json!({})))
                    .await?;
            }
        }
        Ok(())
    }

    fn enrich_stdio_error(&self, err: &str) -> String {
        match &self.transport {
            McpTransport::Stdio(t) => t.with_startup_stderr_summary(err.to_string()),
            McpTransport::Http(_) => err.to_string(),
        }
    }
}

fn parse_tools_list(result: &Value) -> Vec<ToolInsert> {
    result
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|tools| {
            tools
                .iter()
                .map(|tool| ToolInsert {
                    name: tool
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: tool
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    input_schema: tool.get("inputSchema").cloned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 真实适配器:McpClient 实现 McpSession 接口。
/// 连接工厂(StdioHttpConnector)负责构造 McpClient;之后 ServerManager
/// 只通过 trait 接口与它交互,不再知道具体类型。
impl crate::sidecar::services::server_manager::McpSession for McpClient {
    fn list_tools(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<ToolInsert>, String>> + Send + '_>,
    > {
        Box::pin(async move { McpClient::list_tools(self).await })
    }

    fn call_tool<'a>(
        &'a self,
        tool_name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send + 'a>>
    {
        Box::pin(async move { McpClient::call_tool(self, tool_name, args).await })
    }

    fn disconnect(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move { McpClient::disconnect(self).await })
    }

    fn set_request_timeout_ms(&mut self, request_timeout_ms: u32) {
        McpClient::set_request_timeout_ms(self, request_timeout_ms);
    }

    fn alive_receiver(&self) -> Option<tokio::sync::watch::Receiver<bool>> {
        McpClient::alive_receiver(self)
    }
}
