use serde_json::Value;
use std::collections::{HashMap, VecDeque};
#[cfg(all(target_os = "macos", not(test)))]
use std::process::{Command as StdCommand, Stdio as StdStdio};
#[cfg(all(target_os = "macos", not(test)))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(all(target_os = "macos", not(test)))]
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, watch, Mutex as AsyncMutex};

use super::format_timeout_duration;
use crate::sidecar::services::server_log::{redact_sensitive_stderr_values, DiagnosticAttempt};

type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;
type StderrLines = Arc<Mutex<VecDeque<String>>>;

#[cfg(any(windows, test))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x08000000;
const STDERR_SUMMARY_MAX_LINES: usize = 3;
const STDERR_SUMMARY_MAX_CHARS: usize = 240;
#[cfg(all(target_os = "macos", not(test)))]
const LOGIN_SHELL_PATH_TIMEOUT: Duration = Duration::from_millis(1500);

pub struct StdioClientTransport {
    child: Option<Child>,
    stdin_handle: Arc<AsyncMutex<Option<tokio::process::ChildStdin>>>,
    pending: PendingMap,
    stderr_lines: StderrLines,
    next_id: Arc<Mutex<i64>>,
    reader_handle: Option<tokio::task::JoinHandle<()>>,
    alive_rx: watch::Receiver<bool>,
    request_timeout: Duration,
}

impl StdioClientTransport {
    pub async fn spawn(
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        env: HashMap<String, String>,
        request_timeout: Duration,
        diagnostics: DiagnosticAttempt,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(&env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // 即使 close 路径未执行，Drop 也要兜底结束子进程。
            .kill_on_drop(true);

        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        #[cfg(windows)]
        if let Some(flags) = stdio_creation_flags_for_platform(true) {
            cmd.creation_flags(flags);
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn: {e}"))?;
        let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to open stderr")?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let stdin_handle = Arc::new(AsyncMutex::new(Some(stdin)));
        let stderr_lines = Arc::new(Mutex::new(VecDeque::new()));
        let (alive_tx, alive_rx) = watch::channel(true);

        // stdout reader task
        let pending_clone = pending.clone();
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let line = line.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        let parsed: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if let Some(id) = parsed.get("id").and_then(|v| v.as_i64()) {
                            let sender = {
                                let mut map = pending_clone.lock().unwrap();
                                map.remove(&id)
                            };
                            if let Some(sender) = sender {
                                let _ = sender.send(parsed);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            let _ = alive_tx.send(false);
        });

        let stderr_lines_clone = stderr_lines.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                {
                    let mut stderr_lines = stderr_lines_clone.lock().unwrap();
                    record_stderr_line(&mut stderr_lines, &line);
                }
                diagnostics.stderr(&line);
            }
        });

        Ok(Self {
            child: Some(child),
            stdin_handle,
            pending,
            stderr_lines,
            next_id: Arc::new(Mutex::new(1)),
            reader_handle: Some(reader_handle),
            alive_rx,
            request_timeout,
        })
    }

    pub fn with_startup_stderr_summary(&self, err: String) -> String {
        match self.stderr_summary() {
            Some(summary) => format!("{err}. stdio stderr: {summary}"),
            None => err,
        }
    }

    pub async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().unwrap();
            map.insert(id, tx);
        }

        let msg = format!("{}\n", serde_json::to_string(&request).unwrap_or_default());
        {
            let mut stdin_opt = self.stdin_handle.lock().await;
            let stdin = stdin_opt.as_mut().ok_or("stdin closed")?;
            stdin
                .write_all(msg.as_bytes())
                .await
                .map_err(|e| format!("stdin write failed: {e}"))?;
            stdin
                .flush()
                .await
                .map_err(|e| format!("stdin flush failed: {e}"))?;
        }

        match tokio::time::timeout(self.request_timeout, rx).await {
            Ok(Ok(response)) => {
                if let Some(error) = response.get("error") {
                    let msg = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    return Err(msg.to_string());
                }
                Ok(response.get("result").cloned().unwrap_or(Value::Null))
            }
            Ok(Err(_)) => Err("response channel closed".into()),
            Err(_) => {
                let mut map = self.pending.lock().unwrap();
                map.remove(&id);
                Err(format!(
                    "request timed out after {}",
                    format_timeout_duration(self.request_timeout)
                ))
            }
        }
    }

    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
        });
        let msg = format!(
            "{}\n",
            serde_json::to_string(&notification).unwrap_or_default()
        );
        let mut stdin_opt = self.stdin_handle.lock().await;
        let stdin = stdin_opt.as_mut().ok_or("stdin closed")?;
        stdin
            .write_all(msg.as_bytes())
            .await
            .map_err(|e| format!("stdin write failed: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("stdin flush failed: {e}"))
    }

    pub async fn close(&mut self) -> Result<(), String> {
        {
            let mut stdin_opt = self.stdin_handle.lock().await;
            *stdin_opt = None;
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }
        {
            let mut map = self.pending.lock().unwrap();
            map.clear();
        }
        Ok(())
    }

    pub fn stderr_summary(&self) -> Option<String> {
        let lines = self.stderr_lines.lock().unwrap();
        stderr_summary(&lines)
    }

    pub fn alive_receiver(&self) -> watch::Receiver<bool> {
        self.alive_rx.clone()
    }

    pub fn set_request_timeout(&mut self, request_timeout: Duration) {
        self.request_timeout = request_timeout;
    }
}

impl Drop for StdioClientTransport {
    fn drop(&mut self) {
        // 兜底停止 reader task，避免 close 未执行时后台读取继续挂住。
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(any(windows, test))]
fn stdio_creation_flags_for_platform(is_windows: bool) -> Option<u32> {
    is_windows.then_some(WINDOWS_CREATE_NO_WINDOW)
}

fn record_stderr_line(lines: &mut VecDeque<String>, line: &str) {
    let line = summarize_stderr_line(line);
    if line.is_empty() {
        return;
    }
    if lines.len() == STDERR_SUMMARY_MAX_LINES {
        lines.pop_front();
    }
    lines.push_back(line);
}

fn summarize_stderr_line(line: &str) -> String {
    let redacted = redact_sensitive_stderr_values(line);
    let mut chars = redacted.trim().chars();
    let summary: String = chars.by_ref().take(STDERR_SUMMARY_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{summary}…")
    } else {
        summary
    }
}

fn stderr_summary(lines: &VecDeque<String>) -> Option<String> {
    (!lines.is_empty()).then(|| lines.iter().cloned().collect::<Vec<_>>().join(" | "))
}

/// Build the environment for stdio MCP server processes.
pub fn build_stdio_environment(
    parent_env: &HashMap<String, String>,
    server_env: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let login_shell_path = login_shell_path_for_platform();
    build_stdio_environment_with_login_shell_path(
        parent_env,
        server_env,
        login_shell_path.as_deref(),
    )
}

fn build_stdio_environment_with_login_shell_path(
    parent_env: &HashMap<String, String>,
    server_env: Option<&HashMap<String, String>>,
    login_shell_path: Option<&str>,
) -> HashMap<String, String> {
    let is_windows = cfg!(windows);
    let mut env = parent_env.clone();
    if let Some(server_env) = server_env {
        env.extend(server_env.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    let home = env
        .get("HOME")
        .cloned()
        .or_else(|| std::env::var("HOME").ok())
        .or_else(|| std::env::var("USERPROFILE").ok())
        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "/".to_string());

    #[cfg(unix)]
    let default_candidates: &[&str] = &[
        "~/.local/share/mise/shims",
        "~/.local/bin",
        "~/Library/pnpm",
        "~/.cargo/bin",
        "~/.asdf/shims",
        "~/.volta/bin",
        "~/.bun/bin",
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/opt/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ];

    #[cfg(windows)]
    let default_candidates: &[&str] = &[
        "~/AppData/Local/pnpm",
        "~/.cargo/bin",
        "~/AppData/Local/Programs",
        "~/AppData/Local/Microsoft/WinGet/Links",
        "~/scoop/shims",
        "~/AppData/Roaming/npm",
    ];

    let default_entries: Vec<String> = default_candidates
        .iter()
        .map(|p| expand_home(p, &home))
        .collect();

    let mut all_entries = Vec::new();
    if let Some(server_path) =
        server_env.and_then(|server_env| path_value_for_platform(server_env, is_windows))
    {
        all_entries.extend(split_path(server_path));
    }
    if let Some(login_shell_path) = login_shell_path {
        all_entries.extend(split_path(login_shell_path));
    }
    if let Some(parent_path) = path_value_for_platform(parent_env, is_windows) {
        all_entries.extend(split_path(parent_path));
    }
    all_entries.extend(default_entries);

    let mut seen = std::collections::HashSet::new();
    let unique: Vec<String> = all_entries
        .into_iter()
        .filter(|e| seen.insert(e.clone()))
        .collect();

    let separator = if is_windows { ";" } else { ":" };
    env.insert("PATH".to_string(), unique.join(separator));
    env
}

#[cfg(all(target_os = "macos", not(test)))]
fn login_shell_path_for_platform() -> Option<String> {
    static LOGIN_SHELL_PATH: OnceLock<Option<String>> = OnceLock::new();
    LOGIN_SHELL_PATH
        .get_or_init(read_macos_login_shell_path)
        .clone()
}

#[cfg(any(not(target_os = "macos"), test))]
fn login_shell_path_for_platform() -> Option<String> {
    None
}

fn parse_login_shell_path_stdout(stdout: &[u8]) -> Option<String> {
    let stdout = std::str::from_utf8(stdout).ok()?;
    stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(String::from)
}

#[cfg(all(target_os = "macos", not(test)))]
fn read_macos_login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());
    let mut child = StdCommand::new(shell)
        .arg("-l")
        .arg("-i")
        .arg("-c")
        .arg("/usr/bin/printenv PATH")
        .stdout(StdStdio::piped())
        .stderr(StdStdio::null())
        .spawn()
        .ok()?;
    let started_at = Instant::now();

    loop {
        if let Some(_status) = child.try_wait().ok()? {
            let output = child.wait_with_output().ok()?;
            if !output.status.success() {
                return None;
            }
            return parse_login_shell_path_stdout(&output.stdout);
        }
        if started_at.elapsed() >= LOGIN_SHELL_PATH_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn expand_home(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::path::PathBuf::from(home)
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    path.to_string()
}

#[cfg(unix)]
fn split_path(value: &str) -> Vec<String> {
    value
        .split(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(windows)]
fn split_path(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn split_windows_path(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn env_value_for_platform<'a>(
    env: &'a HashMap<String, String>,
    key: &str,
    is_windows: bool,
) -> Option<&'a String> {
    env.get(key).or_else(|| {
        is_windows
            .then(|| {
                env.iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                    .map(|(_, value)| value)
            })
            .flatten()
    })
}

fn path_value_for_platform(env: &HashMap<String, String>, is_windows: bool) -> Option<&String> {
    env_value_for_platform(env, "PATH", is_windows)
}

fn executable_names_for_platform(
    command: &str,
    env: &HashMap<String, String>,
    is_windows: bool,
) -> Vec<String> {
    if !is_windows || std::path::Path::new(command).extension().is_some() {
        return vec![command.to_string()];
    }

    let extensions = env_value_for_platform(env, "PATHEXT", is_windows)
        .map(|value| split_windows_path(value))
        .unwrap_or_else(|| split_windows_path(".EXE;.CMD;.BAT;.COM"));

    std::iter::once(command.to_string())
        .chain(
            extensions
                .into_iter()
                .map(|extension| format!("{command}{}", extension.to_ascii_lowercase())),
        )
        .collect()
}

pub fn find_executable_on_path(command: &str, env: &HashMap<String, String>) -> Option<String> {
    let path = std::path::Path::new(command);
    if path.is_absolute() {
        return if is_executable(command) {
            Some(command.to_string())
        } else {
            None
        };
    }

    if command.contains('/') || command.contains('\\') {
        return None;
    }

    let is_windows = cfg!(windows);
    let path_var = path_value_for_platform(env, is_windows)?;
    let separator = if is_windows { ';' } else { ':' };
    let candidate_names = executable_names_for_platform(command, env, is_windows);
    for dir in path_var.split(separator) {
        for name in &candidate_names {
            let candidate = std::path::PathBuf::from(dir).join(name);
            if is_executable(&candidate.to_string_lossy()) {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn is_executable(path: &str) -> bool {
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &str) -> bool {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !metadata.is_file() {
        return false;
    }
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "exe" | "cmd" | "bat" | "com")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_path_lookup_accepts_path_alias() {
        let env = HashMap::from([("Path".to_string(), r"C:\Tools".to_string())]);

        assert_eq!(
            path_value_for_platform(&env, true).map(String::as_str),
            Some(r"C:\Tools")
        );
    }

    #[test]
    fn windows_executable_names_expand_pathext_for_extensionless_commands() {
        let env = HashMap::from([("PATHEXT".to_string(), ".EXE;.CMD;.BAT;.COM".to_string())]);

        assert_eq!(
            executable_names_for_platform("npx", &env, true),
            vec!["npx", "npx.exe", "npx.cmd", "npx.bat", "npx.com"]
        );
    }

    #[test]
    fn windows_executable_names_keep_explicit_extension() {
        let env = HashMap::from([("PATHEXT".to_string(), ".EXE;.CMD;.BAT;.COM".to_string())]);

        assert_eq!(
            executable_names_for_platform("npx.cmd", &env, true),
            vec!["npx.cmd"]
        );
    }

    #[test]
    fn windows_stdio_processes_hide_console_window() {
        assert_eq!(
            stdio_creation_flags_for_platform(true),
            Some(WINDOWS_CREATE_NO_WINDOW)
        );
    }

    #[test]
    fn non_windows_stdio_processes_do_not_set_creation_flags() {
        assert_eq!(stdio_creation_flags_for_platform(false), None);
    }

    #[test]
    fn stderr_summary_keeps_recent_bounded_lines() {
        let mut lines = VecDeque::new();

        record_stderr_line(&mut lines, "first");
        record_stderr_line(&mut lines, "second");
        record_stderr_line(&mut lines, "third");
        record_stderr_line(&mut lines, "fourth");

        assert_eq!(
            stderr_summary(&lines).as_deref(),
            Some("second | third | fourth")
        );
    }

    #[test]
    fn stderr_summary_redacts_sensitive_values() {
        let mut lines = VecDeque::new();

        record_stderr_line(
            &mut lines,
            "token=abc password: hunter2 apiKey=secret authorization: Bearer abc123; cookie=session",
        );

        assert_eq!(
            stderr_summary(&lines).as_deref(),
            Some(
                "token=[REDACTED] password: [REDACTED] apiKey=[REDACTED] authorization: [REDACTED]; cookie=[REDACTED]"
            )
        );
    }

    #[test]
    fn stderr_summary_redacts_url_userinfo() {
        let mut lines = VecDeque::new();

        record_stderr_line(
            &mut lines,
            "connect https://user:pass@example.com/db then postgres://key@host:5432",
        );

        assert_eq!(
            stderr_summary(&lines).as_deref(),
            Some("connect https://[REDACTED]@example.com/db then postgres://[REDACTED]@host:5432")
        );
    }

    #[test]
    fn stderr_summary_leaves_urls_without_userinfo_untouched() {
        let mut lines = VecDeque::new();

        record_stderr_line(&mut lines, "GET https://example.com/health 200");

        assert_eq!(
            stderr_summary(&lines).as_deref(),
            Some("GET https://example.com/health 200")
        );
    }
}

#[cfg(test)]
mod stdio_env_tests {
    use super::*;
    use std::collections::HashMap;

    #[cfg(unix)]
    #[test]
    fn keeps_server_path_entries_before_process_path_entries() {
        let parent = HashMap::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("PATH".to_string(), "/parent/bin".to_string()),
        ]);
        let server = HashMap::from([("PATH".to_string(), "/server/bin".to_string())]);

        let env = build_stdio_environment(&parent, Some(&server));
        let path = env.get("PATH").expect("PATH set");
        let entries: Vec<&str> = path.split(':').collect();
        let server_idx = entries
            .iter()
            .position(|e| *e == "/server/bin")
            .expect("server path");
        let parent_idx = entries
            .iter()
            .position(|e| *e == "/parent/bin")
            .expect("parent path");
        assert!(
            server_idx < parent_idx,
            "server PATH must precede process PATH"
        );
    }

    #[cfg(unix)]
    #[test]
    fn builds_unix_environment_with_common_cli_paths() {
        let parent = HashMap::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ]);
        let env = build_stdio_environment(&parent, None);
        let path = env.get("PATH").expect("PATH set");
        assert!(path.contains("/opt/homebrew/bin"));
        assert!(path.contains("/usr/local/bin"));
    }

    #[cfg(unix)]
    #[test]
    fn inserts_login_shell_path_between_server_and_parent_path_entries() {
        let parent = HashMap::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("PATH".to_string(), "/parent/bin".to_string()),
        ]);
        let server = HashMap::from([("PATH".to_string(), "/server/bin".to_string())]);

        let env = build_stdio_environment_with_login_shell_path(
            &parent,
            Some(&server),
            Some("/login/bin:/usr/bin"),
        );
        let path = env.get("PATH").expect("PATH set");
        let entries: Vec<&str> = path.split(':').collect();
        let server_idx = entries
            .iter()
            .position(|e| *e == "/server/bin")
            .expect("server path");
        let login_idx = entries
            .iter()
            .position(|e| *e == "/login/bin")
            .expect("login shell path");
        let parent_idx = entries
            .iter()
            .position(|e| *e == "/parent/bin")
            .expect("parent path");

        assert!(server_idx < login_idx);
        assert!(login_idx < parent_idx);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_path_deduplicates_after_server_path_entries() {
        let parent = HashMap::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("PATH".to_string(), "/parent/bin:/shared/bin".to_string()),
        ]);
        let server = HashMap::from([("PATH".to_string(), "/shared/bin:/server/bin".to_string())]);

        let env = build_stdio_environment_with_login_shell_path(
            &parent,
            Some(&server),
            Some("/login/bin:/shared/bin:/parent/bin"),
        );
        let path = env.get("PATH").expect("PATH set");
        let entries: Vec<&str> = path.split(':').collect();

        assert_eq!(
            entries
                .iter()
                .filter(|entry| **entry == "/shared/bin")
                .count(),
            1
        );
        assert!(
            entries.iter().position(|entry| *entry == "/shared/bin")
                < entries.iter().position(|entry| *entry == "/login/bin")
        );
    }

    #[test]
    fn parses_last_non_empty_login_shell_path_stdout_line() {
        assert_eq!(
            parse_login_shell_path_stdout(b"noise\n\n /first/bin:/bin \n /last/bin:/usr/bin \n")
                .as_deref(),
            Some("/last/bin:/usr/bin")
        );
    }

    #[test]
    fn rejects_empty_or_invalid_login_shell_path_stdout() {
        assert_eq!(parse_login_shell_path_stdout(b"\n  \n"), None);
        assert_eq!(parse_login_shell_path_stdout(&[0xff, 0xfe]), None);
    }
}
