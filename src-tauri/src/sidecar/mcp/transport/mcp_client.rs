use super::request_error::RequestError;
use crate::sidecar::db::tool_discovery_repo::ToolInsert;
use crate::sidecar::mcp::transport::http_client::{
    HttpClientTransport, HttpCredentialProvider, HTTP_PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION,
};
use crate::sidecar::mcp::transport::stdio_client::StdioClientTransport;
use crate::sidecar::services::server_log::DiagnosticAttempt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
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
    pub credential_provider: Option<Arc<dyn HttpCredentialProvider>>,
    pub request_timeout_ms: u32,
}

impl McpClient {
    pub async fn ping(&self, timeout: Duration) -> Result<(), RequestError> {
        let result = match &self.transport {
            McpTransport::Stdio(transport) => {
                transport
                    .send_request_with_timeout("ping", None, timeout)
                    .await?
            }
            McpTransport::Http(transport) => {
                transport
                    .send_request_with_timeout(
                        chrono::Utc::now().timestamp_millis(),
                        "ping",
                        None,
                        timeout,
                    )
                    .await?
            }
        };
        if !result.is_object() {
            return Err("Invalid MCP ping response".into());
        }
        Ok(())
    }
    pub async fn connect_stdio(config: StdioConnectConfig) -> Result<Self, RequestError> {
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

    pub async fn connect_http(config: HttpConnectConfig) -> Result<Self, RequestError> {
        let request_timeout = Duration::from_millis(config.request_timeout_ms as u64);
        let transport = match config.credential_provider {
            Some(provider) => HttpClientTransport::with_credential_provider(
                &config.url,
                provider,
                request_timeout,
            ),
            None => HttpClientTransport::new(&config.url, config.headers, request_timeout),
        };
        let mut client = Self {
            transport: McpTransport::Http(transport),
            server_name: config.server_name,
        };
        client.handshake().await?;
        Ok(client)
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInsert>, RequestError> {
        let result = match &self.transport {
            McpTransport::Stdio(t) => t
                .send_request("tools/list", Some(serde_json::json!({})))
                .await
                .map_err(|error| self.enrich_stdio_error(error))?,
            McpTransport::Http(t) => {
                let id = chrono::Utc::now().timestamp_millis();
                t.send_request(id, "tools/list", Some(serde_json::json!({})))
                    .await?
            }
        };
        Ok(parse_tools_list(&result))
    }

    pub async fn call_tool(&self, tool_name: &str, args: Value) -> Result<Value, RequestError> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": args,
        });
        match &self.transport {
            McpTransport::Stdio(t) => t
                .send_request("tools/call", Some(params))
                .await
                .map_err(|error| self.enrich_stdio_error(error)),
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

    async fn handshake(&mut self) -> Result<(), RequestError> {
        let init_params = serde_json::json!({
            "protocolVersion": LEGACY_PROTOCOL_VERSION,
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
                    .map_err(|error| RequestError {
                        message: t.with_startup_stderr_summary(error.message),
                        ..error
                    })?;
                t.send_notification("notifications/initialized", Some(serde_json::json!({})))
                    .await
                    .map_err(|err| t.with_startup_stderr_summary(err))?;
            }
            McpTransport::Http(t) => {
                let mut init_params = init_params;
                let mut first_error = None;
                loop {
                    init_params["protocolVersion"] = t.protocol_version().into();
                    let id = chrono::Utc::now().timestamp_millis();
                    match t
                        .send_request(id, "initialize", Some(init_params.clone()))
                        .await
                    {
                        Ok(result) => {
                            let version = result
                                .get("protocolVersion")
                                .and_then(Value::as_str)
                                .ok_or("MCP initialize response is missing protocolVersion")?;
                            if ![HTTP_PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION].contains(&version)
                            {
                                return Err(format!(
                                    "Unsupported negotiated MCP protocol version: {version}"
                                )
                                .into());
                            }
                            t.set_protocol_version(version);
                            break;
                        }
                        Err(error)
                            if first_error.is_none() && error.is_protocol_version_error() =>
                        {
                            first_error = Some(error);
                            t.restart_with_protocol_version(LEGACY_PROTOCOL_VERSION);
                        }
                        Err(mut error) => {
                            if let Some(first) = first_error {
                                error.message = format!(
                                    "{}; initial version attempt failed: {}",
                                    error.message,
                                    first.diagnostic_message()
                                );
                            }
                            return Err(error);
                        }
                    }
                }
                t.send_notification("notifications/initialized", Some(serde_json::json!({})))
                    .await?;
            }
        }
        Ok(())
    }

    /// 给 stdio 失败补上启动期 stderr 摘要，保留错误类别与下游 code/data。
    fn enrich_stdio_error(&self, error: RequestError) -> RequestError {
        RequestError {
            message: self.stdio_error_message(error.message),
            ..error
        }
    }

    fn stdio_error_message(&self, message: String) -> String {
        match &self.transport {
            McpTransport::Stdio(t) => t.with_startup_stderr_summary(message),
            McpTransport::Http(_) => message,
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
    fn ping(
        &self,
        timeout: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), RequestError>> + Send + '_>>
    {
        Box::pin(McpClient::ping(self, timeout))
    }
    fn list_tools(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<ToolInsert>, String>> + Send + '_>,
    > {
        Box::pin(async move {
            McpClient::list_tools(self)
                .await
                .map_err(|error| error.message)
        })
    }

    fn call_tool<'a>(
        &'a self,
        tool_name: &'a str,
        args: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, RequestError>> + Send + 'a>>
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
