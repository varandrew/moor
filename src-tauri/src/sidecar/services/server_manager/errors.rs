// 面向用户脱敏后的启动错误消息 —— 自由函数,无运行时状态。

use crate::sidecar::mcp::transport::stdio_client::find_executable_on_path;
use std::collections::HashMap;

pub(crate) fn verify_command_available(
    command: &str,
    env: &HashMap<String, String>,
) -> Result<String, String> {
    let path = std::path::Path::new(command);
    if path.is_absolute() {
        if !path.exists() {
            return Err(format!(
                "Command \"{command}\" is not executable while starting this stdio server."
            ));
        }
        return Ok(command.to_string());
    }
    find_executable_on_path(command, env).ok_or_else(|| {
        format!("Command \"{command}\" was not found on PATH while starting this stdio server.")
    })
}

pub fn public_server_start_error_message(err: &str) -> String {
    if let Some(cmd) = extract_missing_command(err) {
        return format!("Command \"{cmd}\" was not found. Configure an absolute command path or update this server environment.");
    }
    if err.contains("not executable") {
        return "Server failed to start. Check that the command path exists and has execute permission.".to_string();
    }
    if let Some(message) = extract_remote_mcp_error_message(err) {
        return format!("Server failed to start: {message}");
    }
    if let Some(summary) = extract_stdio_stderr_summary(err) {
        return format!("Server failed to start: {summary}");
    }
    "Server failed to start. Check logs for details.".to_string()
}

pub(super) fn format_timeout_ms(timeout_ms: u32) -> String {
    if timeout_ms.is_multiple_of(1000) {
        format!("{}s", timeout_ms / 1000)
    } else {
        format!("{timeout_ms}ms")
    }
}

fn extract_missing_command(err: &str) -> Option<String> {
    let re = regex_lite::Regex::new(r#"Command "([^"]+)" was not found"#).ok()?;
    let caps = re.captures(err)?;
    Some(caps[1].to_string())
}

fn extract_stdio_stderr_summary(err: &str) -> Option<&str> {
    err.split_once(". stdio stderr: ")
        .map(|(_, summary)| summary.trim())
        .filter(|summary| !summary.is_empty())
}

fn extract_remote_mcp_error_message(err: &str) -> Option<&str> {
    err.strip_prefix("Remote MCP server error: ")
        .map(str::trim)
        .filter(|message| !message.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolved_path_is_returned_and_missing_command_is_rejected() {
        let env = std::env::vars().collect();
        let resolved = verify_command_available("node", &env)
            .expect("Node.js is required for transport tests");
        assert!(std::path::Path::new(&resolved).is_absolute());
        assert_eq!(verify_command_available(&resolved, &env).unwrap(), resolved);
        assert!(verify_command_available("moor-missing-command-2183720", &env).is_err());
    }
}
