// 连接 seam:McpSession/McpConnector 接口与真实 stdio/http 适配器。

use super::errors::verify_command_available;
use super::{ServerTimeouts, StoredServerConfig};
use crate::sidecar::db::tool_discovery_repo::ToolInsert;
use crate::sidecar::mcp::transport::mcp_client::{
    HttpConnectConfig, McpClient, StdioConnectConfig,
};
use crate::sidecar::mcp::transport::request_error::RequestError;
use crate::sidecar::mcp::transport::stdio_client::build_stdio_environment;
use crate::sidecar::services::server_log::DiagnosticAttempt;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// 运行时 MCP 会话接口。连接建立后,ServerManager 通过它列工具、调工具、断开、
/// 读存活信号。真实适配器是 McpClient;测试里用假适配器实现它。
/// async 方法手写 BoxFuture,让 trait 可作 `dyn McpSession` 用。
pub trait McpSession: Send {
    fn ping(
        &self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), RequestError>> + Send + '_>>;
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

        let command = verify_command_available(command, &env)?;

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
            command,
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::sidecar::mcp::transport::stdio_client::find_executable_on_path;
    use crate::sidecar::services::server_log::ServerDiagnostics;

    #[tokio::test]
    async fn resolved_batch_command_performs_handshake_with_spaces_and_path_alias() {
        let dir = std::env::temp_dir().join(format!("moor windows {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let parent: HashMap<String, String> = std::env::vars().collect();
        let node = find_executable_on_path("node", &parent)
            .expect("Node.js is required for the Windows integration test");
        let node_dir = std::path::Path::new(&node)
            .parent()
            .unwrap()
            .to_string_lossy();
        let script = dir.join("server with spaces.mjs");
        std::fs::write(&script, r#"import readline from 'node:readline';
for await (const line of readline.createInterface({ input: process.stdin })) {
 const request = JSON.parse(line);
 if (request.id === undefined) continue;
 const result = request.method === 'initialize' ? { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'batch', version: '1.0.0' } } : request.method === 'tools/list' ? { tools: [] } : {};
 process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n');
}"#).unwrap();
        for extension in ["cmd"] {
            std::fs::write(
                dir.join(format!("npx.{extension}")),
                "@echo off\r\nnode %*\r\n",
            )
            .unwrap();
        }
        for command in [
            "npx".to_string(),
            dir.join("npx.cmd").to_string_lossy().into_owned(),
            dir.join("npx.bat").to_string_lossy().into_owned(),
        ] {
            if command.ends_with(".bat") {
                std::fs::write(dir.join("npx.bat"), "@echo off\r\nnode %*\r\n").unwrap();
            }
            let config = StoredServerConfig {
                name: "windows".into(),
                connection_type: "stdio".into(),
                command: Some(command.clone()),
                args: Some(serde_json::json!([script.to_string_lossy()])),
                url: None,
                env: Some(
                    serde_json::json!({"pAtH": format!("{};{}", dir.to_string_lossy(), node_dir), "PATHEXT": ".CMD;.EXE;.BAT"}),
                ),
                headers: None,
                working_dir: Some(dir.to_string_lossy().into_owned()),
            };
            let diagnostics = ServerDiagnostics::default().begin_attempt("windows", "npx");
            let (_, mut client) = StdioHttpConnector::connect_stdio(
                &config,
                ServerTimeouts {
                    request_ms: 30_000,
                    start_ms: 30_000,
                },
                diagnostics,
            )
            .await
            .expect("resolved batch command should initialize");
            client
                .ping(Duration::from_secs(5))
                .await
                .expect("batch server should respond to ping");
            client.disconnect().await.unwrap();
            assert_eq!(config.command.as_deref(), Some(command.as_str()));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
