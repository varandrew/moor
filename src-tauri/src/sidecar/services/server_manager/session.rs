// 连接 seam:McpSession/McpConnector 接口与真实 stdio/http 适配器。

use super::errors::verify_command_available;
use super::{ServerTimeouts, StoredServerConfig};
use crate::sidecar::db::tool_discovery_repo::ToolInsert;
use crate::sidecar::mcp::transport::mcp_client::{
    HttpConnectConfig, McpClient, StdioConnectConfig,
};
use crate::sidecar::mcp::transport::stdio_client::build_stdio_environment;
use crate::sidecar::services::server_log::DiagnosticAttempt;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// 运行时 MCP 会话接口。连接建立后,ServerManager 通过它列工具、调工具、断开、
/// 读存活信号。真实适配器是 McpClient;测试里用假适配器实现它。
/// async 方法手写 BoxFuture,让 trait 可作 `dyn McpSession` 用。
pub trait McpSession: Send {
    #[allow(dead_code)]
    fn list_tools(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ToolInsert>, String>> + Send + '_>>;
    fn call_tool<'a>(
        &'a self,
        tool_name: &'a str,
        args: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;
    fn disconnect(&mut self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
    fn set_request_timeout_ms(&mut self, request_timeout_ms: u32);
    fn alive_receiver(&self) -> Option<tokio::sync::watch::Receiver<bool>>;
}

/// 连接工厂返回的会话盒。类型别名消除 clippy::type_complexity 警告,
/// 也让 connect 签名更可读。
pub type BoxedConnectFuture<'a> = Pin<
    Box<dyn Future<Output = Result<(Vec<ToolInsert>, Box<dyn McpSession>), String>> + Send + 'a>,
>;

/// 连接工厂接口。把"怎么从存储配置建立会话"藏到接缝背后。
/// 真实适配器按 connection_type 分派到 stdio/http 连接并构造 McpClient;
/// 测试可注入假工厂,返回预设的 (tools, 假会话),让 start_server 的 token
/// 竞争、接受/拒绝、death watcher 在没有真实子进程的情况下可测。
pub trait McpConnector: Send + Sync {
    fn connect<'a>(
        &'a self,
        config: &'a StoredServerConfig,
        timeouts: ServerTimeouts,
        diagnostics: DiagnosticAttempt,
    ) -> BoxedConnectFuture<'a>;
}

/// 真实连接工厂适配器。按 connection_type 分派到 stdio/http 连接,
/// 在内部完成环境变量构建、命令可用性检查、header 解析、MCP 握手,
/// 然后返回一个装在 Box<dyn McpSession> 里的 McpClient。
pub(super) struct StdioHttpConnector;

impl McpConnector for StdioHttpConnector {
    fn connect<'a>(
        &'a self,
        config: &'a StoredServerConfig,
        timeouts: ServerTimeouts,
        diagnostics: DiagnosticAttempt,
    ) -> BoxedConnectFuture<'a> {
        Box::pin(async move {
            match config.connection_type.as_str() {
                "stdio" => Self::connect_stdio(config, timeouts, diagnostics).await,
                "http" => Self::connect_http(config, timeouts).await,
                other => Err(format!("Unknown connection type: {other}")),
            }
        })
    }
}

impl StdioHttpConnector {
    async fn connect_stdio(
        config: &StoredServerConfig,
        timeouts: ServerTimeouts,
        diagnostics: DiagnosticAttempt,
    ) -> Result<(Vec<ToolInsert>, Box<dyn McpSession>), String> {
        let command = config
            .command
            .as_deref()
            .ok_or("stdio server requires command")?;

        let parent_env: HashMap<String, String> = std::env::vars().collect();
        let server_env = config
            .env
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let env = build_stdio_environment(&parent_env, server_env.as_ref());

        verify_command_available(command, &env)?;

        let args: Vec<String> = config
            .args
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut client = McpClient::connect_stdio(StdioConnectConfig {
            server_name: config.name.clone(),
            command: command.to_string(),
            args,
            cwd: config.working_dir.clone(),
            env,
            request_timeout_ms: timeouts.start_ms,
            diagnostics,
        })
        .await?;

        let tools = client.list_tools().await?;
        client.set_request_timeout_ms(timeouts.request_ms);

        Ok((tools, Box::new(client)))
    }

    async fn connect_http(
        config: &StoredServerConfig,
        timeouts: ServerTimeouts,
    ) -> Result<(Vec<ToolInsert>, Box<dyn McpSession>), String> {
        let url = config.url.as_deref().ok_or("http server requires url")?;

        let headers = config
            .headers
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let env = config
            .env
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let headers = crate::sidecar::mcp::transport::http_client::resolve_http_headers(
            Some(&headers),
            Some(&env),
        );

        let mut client = McpClient::connect_http(HttpConnectConfig {
            server_name: config.name.clone(),
            url: url.to_string(),
            headers,
            request_timeout_ms: timeouts.start_ms,
        })
        .await?;

        let tools = client.list_tools().await?;
        client.set_request_timeout_ms(timeouts.request_ms);

        Ok((tools, Box::new(client)))
    }
}
