use crate::sidecar::db::{settings_repo, Database};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{fs, path::Path};

const SETTINGS_FILE: &str = "settings.json";
pub const MCP_TIMEOUT_MS_MIN: u32 = 5_000;
pub const MCP_TIMEOUT_MS_MAX: u32 = 300_000;
pub const MCP_TIMEOUT_MS_DEFAULT: u32 = 30_000;
pub const MCP_SESSION_IDLE_TTL_MS_MIN: u32 = 300_000;
pub const MCP_SESSION_IDLE_TTL_MS_MAX: u32 = 86_400_000;
pub const MCP_SESSION_IDLE_TTL_MS_DEFAULT: u32 = 3_600_000;

/// Distinguishes client input errors (HTTP 400) from internal failures (HTTP 500).
#[derive(Debug)]
pub enum SettingsError {
    Validation(String),
    Internal(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::Validation(m) | SettingsError::Internal(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub auto_start_on_login: bool,
    pub auto_start_servers_on_launch: bool,
    pub minimize_to_tray_on_close: bool,
    pub hide_dock_icon_on_close: bool,
    pub show_window_on_launch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: String,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSettings {
    pub log_retention_days: u16,
    pub enable_audit_logging: bool,
    pub sidecar_port: u16,
    pub allow_lan_mcp_access: bool,
    pub mcp_request_timeout_ms: u32,
    pub mcp_server_start_timeout_ms: u32,
    pub mcp_session_idle_ttl_ms: u32,
    pub mcp_health_checks_enabled: bool,
    pub mcp_health_check_interval_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub advanced: AdvancedSettings,
}

pub fn default_settings() -> Settings {
    Settings {
        version: 1,
        general: GeneralSettings {
            auto_start_on_login: false,
            auto_start_servers_on_launch: false,
            minimize_to_tray_on_close: true,
            hide_dock_icon_on_close: false,
            show_window_on_launch: true,
        },
        appearance: AppearanceSettings {
            theme: "system".to_string(),
            locale: "system".to_string(),
        },
        advanced: AdvancedSettings {
            log_retention_days: 30,
            enable_audit_logging: true,
            sidecar_port: 9223,
            allow_lan_mcp_access: false,
            mcp_request_timeout_ms: MCP_TIMEOUT_MS_DEFAULT,
            mcp_server_start_timeout_ms: MCP_TIMEOUT_MS_DEFAULT,
            mcp_session_idle_ttl_ms: MCP_SESSION_IDLE_TTL_MS_DEFAULT,
            mcp_health_checks_enabled: false,
            mcp_health_check_interval_seconds: 60,
        },
    }
}

pub fn settings_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(SETTINGS_FILE)
}

pub fn read_settings_file(data_dir: &Path) -> Settings {
    let path = settings_path(data_dir);
    let Ok(content) = fs::read_to_string(path) else {
        return default_settings();
    };
    merge_settings_value(
        default_settings(),
        serde_json::from_str(&content).unwrap_or(Value::Null),
    )
    .unwrap_or_else(|_| default_settings())
}

// 键清单由序列化结构派生——不再手写枚举 `("group.camelKey", value)` 行。
fn settings_to_db_entries(settings: &Settings) -> Result<Vec<(String, String)>, String> {
    let value = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    flatten_to_entries(&value, String::new(), &mut entries);
    Ok(entries)
}

fn flatten_to_entries(value: &Value, prefix: String, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let dotted = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_to_entries(inner, dotted, out);
            }
        }
        leaf => out.push((prefix, leaf.to_string())),
    }
}

fn insert_nested_setting(root: &mut Map<String, Value>, key: &str, value: Value) {
    let parts = key.split('.').collect::<Vec<_>>();
    if parts.is_empty() {
        return;
    }

    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        let entry = current
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry
            .as_object_mut()
            .expect("entry was normalized to object");
    }

    current.insert(parts[parts.len() - 1].to_string(), value);
}

fn db_entries_to_settings(entries: std::collections::HashMap<String, String>) -> Settings {
    let mut raw = Map::new();
    for (key, value) in entries {
        let parsed = serde_json::from_str(&value).unwrap_or(Value::String(value));
        insert_nested_setting(&mut raw, &key, parsed);
    }
    merge_settings_value(default_settings(), Value::Object(raw))
        .unwrap_or_else(|_| default_settings())
}

fn persist(db: &Database, settings: &Settings) -> Result<(), String> {
    let entries = settings_to_db_entries(settings)?;
    settings_repo::save_entries(db, &entries)
}

pub fn init_settings(db: &Database, data_dir: &Path) -> Result<Settings, String> {
    let entries = settings_repo::load_entries(db)?;
    if !entries.is_empty() {
        return Ok(db_entries_to_settings(entries));
    }

    let settings = read_settings_file(data_dir);
    persist(db, &settings)?;
    Ok(settings)
}

pub fn get_settings(db: &Database) -> Result<Settings, String> {
    let entries = settings_repo::load_entries(db)?;
    if entries.is_empty() {
        return Ok(default_settings());
    }
    Ok(db_entries_to_settings(entries))
}

pub fn update_settings(db: &Database, patch: Value) -> Result<Settings, SettingsError> {
    let current = get_settings(db).map_err(SettingsError::Internal)?;
    let updated = merge_settings_value(current, patch).map_err(SettingsError::Validation)?;
    persist(db, &updated).map_err(SettingsError::Internal)?;
    Ok(updated)
}

pub fn reset_settings(db: &Database) -> Result<Settings, String> {
    let settings = default_settings();
    persist(db, &settings)?;
    Ok(settings)
}

pub fn merge_settings_value(base: Settings, patch: Value) -> Result<Settings, String> {
    if patch.is_null() {
        return Ok(base);
    }
    let mut merged = serde_json::to_value(&base).map_err(|e| e.to_string())?;
    let patch = patch
        .as_object()
        .ok_or_else(|| "Invalid settings payload: expected an object".to_string())?;
    merge_objects(
        merged
            .as_object_mut()
            .expect("settings serializes to an object"),
        patch,
    );
    validate_settings_value(&merged)?;
    serde_json::from_value(merged).map_err(|e| format!("Invalid settings payload: {e}"))
}

// Option<T> 语义的深合并：patch 中的 null 视为缺省，对象递归，其余直接覆盖。
fn merge_objects(base: &mut Map<String, Value>, patch: &Map<String, Value>) {
    for (key, value) in patch {
        if value.is_null() {
            continue;
        }
        match (base.get_mut(key), value) {
            (Some(Value::Object(base_inner)), Value::Object(patch_inner)) => {
                merge_objects(base_inner, patch_inner);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

// 数值范围收拢在一张表；新增一个有界设置只需加一行。
const NUMERIC_BOUNDS: &[(&str, u64, u64)] = &[
    ("advanced.mcpHealthCheckIntervalSeconds", 30, 600),
    ("advanced.logRetentionDays", 0, 365),
    ("advanced.sidecarPort", 1024, 65535),
    (
        "advanced.mcpRequestTimeoutMs",
        MCP_TIMEOUT_MS_MIN as u64,
        MCP_TIMEOUT_MS_MAX as u64,
    ),
    (
        "advanced.mcpServerStartTimeoutMs",
        MCP_TIMEOUT_MS_MIN as u64,
        MCP_TIMEOUT_MS_MAX as u64,
    ),
    (
        "advanced.mcpSessionIdleTtlMs",
        MCP_SESSION_IDLE_TTL_MS_MIN as u64,
        MCP_SESSION_IDLE_TTL_MS_MAX as u64,
    ),
];

fn setting_at<'a>(value: &'a Value, dotted: &str) -> Option<&'a Value> {
    dotted
        .split('.')
        .try_fold(value, |current, part| current.get(part))
}

fn validate_settings_value(value: &Value) -> Result<(), String> {
    if !matches!(
        setting_at(value, "appearance.locale").and_then(Value::as_str),
        Some("system" | "zh-CN" | "en")
    ) {
        return Err("appearance.locale must be system, zh-CN, or en".to_string());
    }
    if setting_at(value, "version").and_then(Value::as_u64) == Some(0) {
        return Err("version must be at least 1".to_string());
    }
    if !matches!(
        setting_at(value, "appearance.theme").and_then(Value::as_str),
        Some("light" | "dark" | "system")
    ) {
        return Err("appearance.theme must be light, dark, or system".to_string());
    }
    for (path, min, max) in NUMERIC_BOUNDS {
        // 非数值叶子会以反序列化错误的形式暴露。
        if let Some(number) = setting_at(value, path).and_then(Value::as_u64) {
            if number < *min || number > *max {
                return Err(format!("{path} must be between {min} and {max}"));
            }
        }
    }
    Ok(())
}

pub fn audit_logging_enabled(db: &Database) -> bool {
    get_settings(db)
        .map(|settings| settings.advanced.enable_audit_logging)
        .unwrap_or_else(|_| default_settings().advanced.enable_audit_logging)
}

/// 审计日志滚动保留天数；读取失败时退回默认值。0 表示永久保留。
pub fn audit_retention_days(db: &Database) -> u16 {
    get_settings(db)
        .map(|settings| settings.advanced.log_retention_days)
        .unwrap_or_else(|_| default_settings().advanced.log_retention_days)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn validate_settings(settings: &Settings) -> Result<(), String> {
        validate_settings_value(&serde_json::to_value(settings).map_err(|e| e.to_string())?)
    }
    use crate::sidecar::db::Database;
    use std::time::SystemTime;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-settings-{test_name}-{timestamp}"))
    }

    fn test_db(data_dir: &Path) -> Database {
        fs::create_dir_all(data_dir).expect("failed to create temp settings dir");
        let db = Database::open(&data_dir.join("moor.db")).expect("failed to open settings db");
        db.run_migrations().expect("failed to migrate settings db");
        db
    }

    #[test]
    fn locale_and_health_defaults_preserve_existing_settings() {
        let data_dir = temp_data_dir("locale-health");
        let db = test_db(&data_dir);
        let legacy = update_settings(&db, serde_json::json!({ "general": { "autoStartOnLogin": true }, "advanced": { "sidecarPort": 9333 } })).unwrap();
        assert_eq!(legacy.appearance.locale, "system");
        assert!(!legacy.advanced.mcp_health_checks_enabled);
        assert_eq!(legacy.advanced.mcp_health_check_interval_seconds, 60);
        update_settings(&db, serde_json::json!({ "appearance": { "locale": "zh-CN" }, "advanced": { "mcpHealthChecksEnabled": true, "mcpHealthCheckIntervalSeconds": 30 } })).unwrap();
        let persisted = get_settings(&db).unwrap();
        assert_eq!(persisted.appearance.locale, "zh-CN");
        assert!(persisted.general.auto_start_on_login);
        assert_eq!(persisted.advanced.sidecar_port, 9333);
        for invalid in [
            serde_json::json!({"appearance":{"locale":"de"}}),
            serde_json::json!({"advanced":{"mcpHealthCheckIntervalSeconds":29}}),
            serde_json::json!({"advanced":{"mcpHealthCheckIntervalSeconds":601}}),
            serde_json::json!({"advanced":{"mcpHealthCheckIntervalSeconds":30.5}}),
        ] {
            assert!(update_settings(&db, invalid).is_err());
        }
        assert_eq!(get_settings(&db).unwrap(), persisted);
        drop(db);
        let reopened = test_db(&data_dir);
        assert_eq!(get_settings(&reopened).unwrap(), persisted);
        drop(reopened);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn updates_allow_lan_mcp_access() {
        let data_dir = temp_data_dir("lan-access");
        let db = test_db(&data_dir);

        let updated = update_settings(
            &db,
            serde_json::json!({ "advanced": { "allowLanMcpAccess": true } }),
        )
        .expect("settings update should succeed");

        assert!(updated.advanced.allow_lan_mcp_access);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn fresh_store_returns_defaults() {
        let data_dir = temp_data_dir("fresh");
        let db = test_db(&data_dir);

        let settings = init_settings(&db, &data_dir).expect("settings should initialize");

        assert_eq!(settings, default_settings());
        assert_eq!(
            get_settings(&db).expect("settings should load"),
            default_settings()
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn migrates_settings_json_once_and_keeps_database_as_source_of_truth() {
        let data_dir = temp_data_dir("patch");
        fs::create_dir_all(&data_dir).expect("failed to create temp settings dir");
        fs::write(
            settings_path(&data_dir),
            r#"{
              "general": { "minimizeToTrayOnClose": false },
              "advanced": { "sidecarPort": 9333 }
            }"#,
        )
        .expect("failed to write migration source");
        let db = test_db(&data_dir);

        let migrated = init_settings(&db, &data_dir).expect("settings should migrate");
        let updated = update_settings(
            &db,
            serde_json::json!({
                "general": { "minimizeToTrayOnClose": true },
                "advanced": { "sidecarPort": 9444 }
            }),
        )
        .expect("settings update should succeed");

        assert!(!migrated.general.minimize_to_tray_on_close);
        assert_eq!(migrated.advanced.sidecar_port, 9333);
        assert!(updated.general.minimize_to_tray_on_close);
        assert!(updated.general.show_window_on_launch);
        assert_eq!(updated.advanced.sidecar_port, 9444);
        assert!(
            !read_settings_file(&data_dir)
                .general
                .minimize_to_tray_on_close
        );
        assert_eq!(read_settings_file(&data_dir).advanced.sidecar_port, 9333);
        assert_eq!(get_settings(&db).expect("settings should load"), updated);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn rejects_invalid_port_updates() {
        let data_dir = temp_data_dir("invalid-port");
        let db = test_db(&data_dir);
        init_settings(&db, &data_dir).expect("settings should initialize");

        let err = update_settings(
            &db,
            serde_json::json!({ "advanced": { "sidecarPort": 80 } }),
        )
        .expect_err("invalid port should fail");
        assert!(matches!(
            err,
            SettingsError::Validation(ref m) if m.contains("advanced.sidecarPort")
        ));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn merges_mcp_timeout_settings() {
        let updated = merge_settings_value(
            default_settings(),
            serde_json::json!({
                "advanced": {
                    "mcpRequestTimeoutMs": MCP_TIMEOUT_MS_MIN,
                    "mcpServerStartTimeoutMs": MCP_TIMEOUT_MS_MAX
                }
            }),
        )
        .expect("timeout settings should merge");

        assert_eq!(updated.advanced.mcp_request_timeout_ms, MCP_TIMEOUT_MS_MIN);
        assert_eq!(
            updated.advanced.mcp_server_start_timeout_ms,
            MCP_TIMEOUT_MS_MAX
        );
    }

    #[test]
    fn rejects_invalid_mcp_timeout_settings() {
        let mut settings = default_settings();
        settings.advanced.mcp_request_timeout_ms = MCP_TIMEOUT_MS_MIN - 1;
        let err = validate_settings(&settings).expect_err("request timeout should fail");
        assert!(err.contains("advanced.mcpRequestTimeoutMs"));

        let mut settings = default_settings();
        settings.advanced.mcp_server_start_timeout_ms = MCP_TIMEOUT_MS_MAX + 1;
        let err = validate_settings(&settings).expect_err("start timeout should fail");
        assert!(err.contains("advanced.mcpServerStartTimeoutMs"));
    }

    #[test]
    fn audit_logging_enabled_reads_database_settings() {
        let data_dir = temp_data_dir("audit");
        let db = test_db(&data_dir);
        init_settings(&db, &data_dir).expect("settings should initialize");

        update_settings(
            &db,
            serde_json::json!({ "advanced": { "enableAuditLogging": false } }),
        )
        .expect("settings update should succeed");

        assert!(!audit_logging_enabled(&db));
        let _ = fs::remove_dir_all(data_dir);
    }
}
