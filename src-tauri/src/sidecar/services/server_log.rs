// Server Diagnostics 集中管理启动尝试、脱敏和尽力写入；日志失败不影响 Server Runtime。
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

const STDERR_REDACTED: &str = "[REDACTED]";

#[derive(Default)]
pub struct ServerDiagnostics {
    logs_dir: Option<PathBuf>,
    attempts: Mutex<HashMap<String, Weak<Mutex<bool>>>>,
}

#[derive(Clone)]
pub struct DiagnosticAttempt {
    path: Option<PathBuf>,
    active: Arc<Mutex<bool>>,
}

impl ServerDiagnostics {
    pub fn new(logs_dir: PathBuf) -> Self {
        Self {
            logs_dir: Some(logs_dir),
            ..Self::default()
        }
    }

    pub fn begin_attempt(&self, server_id: &str, command_line: &str) -> DiagnosticAttempt {
        let mut attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        attempts.retain(|_, attempt| attempt.strong_count() > 0);
        if let Some(previous) = attempts.get(server_id).and_then(Weak::upgrade) {
            // 等旧写入完成再失效；新文件截断后，旧 reader 只能更新自己的摘要。
            *previous.lock().unwrap_or_else(|e| e.into_inner()) = false;
        }
        let attempt = DiagnosticAttempt {
            path: self.logs_dir.as_ref().map(|dir| log_path(dir, server_id)),
            active: Arc::new(Mutex::new(true)),
        };
        if let Some(path) = &attempt.path {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(mut file) = std::fs::File::create(path) {
                let _ = writeln!(
                    file,
                    "[{}] === Start attempt: {} ===",
                    timestamp(),
                    redact_sensitive_stderr_values(command_line)
                );
            }
        }
        attempts.insert(server_id.to_string(), Arc::downgrade(&attempt.active));
        attempt
    }
}

impl DiagnosticAttempt {
    pub fn append_event(&self, message: &str) {
        let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if *active {
            self.write(&redact_sensitive_stderr_values(message));
        }
    }

    pub fn stderr(&self, line: &str) {
        let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if *active {
            let redacted = redact_sensitive_stderr_values(line);
            self.write(&format!("[stderr] {redacted}"));
            tracing::warn!(target: "mcp::stdio::stderr", "{}", redacted);
        }
    }

    fn write(&self, message: &str) {
        let Some(path) = &self.path else { return };
        // 每次重开可恢复被用户删除的日志；调用方不承担文件错误。
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            let _ = writeln!(file, "[{}] {}", timestamp(), message);
        }
    }
}

pub fn log_path(logs_dir: &Path, server_id: &str) -> PathBuf {
    logs_dir.join(format!("{server_id}.log"))
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn redact_sensitive_stderr_values(line: &str) -> String {
    let redacted = sensitive_stderr_key_regex()
        .replace_all(line, |caps: &regex_lite::Captures<'_>| {
            format!("{}{}{}", &caps[1], &caps[2], STDERR_REDACTED)
        });
    let redacted = authorization_stderr_regex()
        .replace_all(&redacted, |caps: &regex_lite::Captures<'_>| {
            format!("{}{}{}", &caps[1], &caps[2], STDERR_REDACTED)
        });
    url_userinfo_stderr_regex()
        .replace_all(&redacted, |caps: &regex_lite::Captures<'_>| {
            format!("{}{}@", &caps[1], STDERR_REDACTED)
        })
        .to_string()
}
fn sensitive_stderr_key_regex() -> &'static regex_lite::Regex {
    static REGEX: OnceLock<regex_lite::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b([A-Za-z0-9_-]*(?:token|password|secret|api[_-]?(?:key|token)|cookie)[A-Za-z0-9_-]*)\b(\s*[:=]\s*)([^\s,;]+)",
        )
        .expect("sensitive stderr key regex should compile")
    })
}

fn authorization_stderr_regex() -> &'static regex_lite::Regex {
    static REGEX: OnceLock<regex_lite::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex_lite::Regex::new(r"(?i)\b(authorization)\b(\s*[:=]\s*)([^,;]+)")
            .expect("authorization stderr regex should compile")
    })
}

// URL userinfo(://user:pass@ 或 ://key@)整体脱敏:key=value 正则覆盖不到该凭据形态。
fn url_userinfo_stderr_regex() -> &'static regex_lite::Regex {
    static REGEX: OnceLock<regex_lite::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex_lite::Regex::new(r"(?i)(://)([^/@\s]+(?::[^/@\s]*)?)@")
            .expect("url userinfo stderr regex should compile")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_logs_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "moor-server-log-{test_name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn log_path_appends_server_id_with_log_extension() {
        assert_eq!(
            log_path(Path::new("/data/logs"), "abc-123"),
            PathBuf::from("/data/logs/abc-123.log")
        );
    }

    #[test]
    fn begin_attempt_creates_dir_and_truncates_existing_file() {
        let dir = temp_logs_dir("begin-attempt");
        let path = log_path(&dir, "srv1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "stale contents from a previous attempt").unwrap();

        let diagnostics = ServerDiagnostics::new(dir.clone());
        let _attempt = diagnostics.begin_attempt("srv1", "npx -y some-mcp");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("=== Start attempt: npx -y some-mcp ==="));
        assert!(!contents.contains("stale contents"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn append_event_creates_missing_file_and_appends() {
        let dir = temp_logs_dir("append-event");
        std::fs::create_dir_all(&dir).unwrap();
        let path = log_path(&dir, "srv2");

        let diagnostics = ServerDiagnostics::new(dir.clone());
        let attempt = diagnostics.begin_attempt("srv2", "some-mcp");
        std::fs::remove_file(&path).unwrap();
        attempt.append_event("Start failed: boom");
        attempt.append_event("exited unexpectedly");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Start failed: boom"));
        assert!(contents.contains("exited unexpectedly"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
