pub mod http_client;
pub mod mcp_client;
pub mod mcp_session;
pub mod stdio_client;
pub mod streamable_http_server;

use std::time::Duration;

pub(crate) fn format_timeout_duration(timeout: Duration) -> String {
    if timeout.as_millis().is_multiple_of(1000) {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    }
}
