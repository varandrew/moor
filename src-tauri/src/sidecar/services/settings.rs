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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub advanced: AdvancedSettings,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PartialGeneralSettings {
    auto_start_on_login: Option<bool>,
    auto_start_servers_on_launch: Option<bool>,
    minimize_to_tray_on_close: Option<bool>,
    hide_dock_icon_on_close: Option<bool>,
    show_window_on_launch: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PartialAppearanceSettings {
    theme: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PartialAdvancedSettings {
    log_retention_days: Option<u16>,
    enable_audit_logging: Option<bool>,
    sidecar_port: Option<u16>,
    allow_lan_mcp_access: Option<bool>,
    mcp_request_timeout_ms: Option<u32>,
    mcp_server_start_timeout_ms: Option<u32>,
    mcp_session_idle_ttl_ms: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PartialSettings {
    version: Option<u32>,
    general: Option<PartialGeneralSettings>,
    appearance: Option<PartialAppearanceSettings>,
    advanced: Option<PartialAdvancedSettings>,
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
        },
        advanced: AdvancedSettings {
            log_retention_days: 30,
            enable_audit_logging: true,
            sidecar_port: 9223,
            allow_lan_mcp_access: false,
            mcp_request_timeout_ms: MCP_TIMEOUT_MS_DEFAULT,
            mcp_server_start_timeout_ms: MCP_TIMEOUT_MS_DEFAULT,
            mcp_session_idle_ttl_ms: MCP_SESSION_IDLE_TTL_MS_DEFAULT,
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

fn settings_to_db_entries(settings: &Settings) -> Result<Vec<(&'static str, String)>, String> {
    Ok(vec![
        (
            "general.autoStartOnLogin",
            serde_json::to_string(&settings.general.auto_start_on_login)
                .map_err(|e| e.to_string())?,
        ),
        (
            "general.autoStartServersOnLaunch",
            serde_json::to_string(&settings.general.auto_start_servers_on_launch)
                .map_err(|e| e.to_string())?,
        ),
        (
            "general.minimizeToTrayOnClose",
            serde_json::to_string(&settings.general.minimize_to_tray_on_close)
                .map_err(|e| e.to_string())?,
        ),
        (
            "general.hideDockIconOnClose",
            serde_json::to_string(&settings.general.hide_dock_icon_on_close)
                .map_err(|e| e.to_string())?,
        ),
        (
            "general.showWindowOnLaunch",
            serde_json::to_string(&settings.general.show_window_on_launch)
                .map_err(|e| e.to_string())?,
        ),
        (
            "appearance.theme",
            serde_json::to_string(&settings.appearance.theme).map_err(|e| e.to_string())?,
        ),
        (
            "advanced.logRetentionDays",
            serde_json::to_string(&settings.advanced.log_retention_days)
                .map_err(|e| e.to_string())?,
        ),
        (
            "advanced.enableAuditLogging",
            serde_json::to_string(&settings.advanced.enable_audit_logging)
                .map_err(|e| e.to_string())?,
        ),
        (
            "advanced.sidecarPort",
            serde_json::to_string(&settings.advanced.sidecar_port).map_err(|e| e.to_string())?,
        ),
        (
            "advanced.allowLanMcpAccess",
            serde_json::to_string(&settings.advanced.allow_lan_mcp_access)
                .map_err(|e| e.to_string())?,
        ),
        (
            "advanced.mcpRequestTimeoutMs",
            serde_json::to_string(&settings.advanced.mcp_request_timeout_ms)
                .map_err(|e| e.to_string())?,
        ),
        (
            "advanced.mcpServerStartTimeoutMs",
            serde_json::to_string(&settings.advanced.mcp_server_start_timeout_ms)
                .map_err(|e| e.to_string())?,
        ),
        (
            "advanced.mcpSessionIdleTtlMs",
            serde_json::to_string(&settings.advanced.mcp_session_idle_ttl_ms)
                .map_err(|e| e.to_string())?,
        ),
    ])
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

pub fn merge_settings_value(mut base: Settings, value: Value) -> Result<Settings, String> {
    if value.is_null() {
        return Ok(base);
    }
    let partial: PartialSettings =
        serde_json::from_value(value).map_err(|e| format!("Invalid settings payload: {e}"))?;
    if let Some(version) = partial.version {
        base.version = version;
    }
    if let Some(general) = partial.general {
        if let Some(value) = general.auto_start_on_login {
            base.general.auto_start_on_login = value;
        }
        if let Some(value) = general.auto_start_servers_on_launch {
            base.general.auto_start_servers_on_launch = value;
        }
        if let Some(value) = general.minimize_to_tray_on_close {
            base.general.minimize_to_tray_on_close = value;
        }
        if let Some(value) = general.hide_dock_icon_on_close {
            base.general.hide_dock_icon_on_close = value;
        }
        if let Some(value) = general.show_window_on_launch {
            base.general.show_window_on_launch = value;
        }
    }
    if let Some(appearance) = partial.appearance {
        if let Some(theme) = appearance.theme {
            base.appearance.theme = theme;
        }
    }
    if let Some(advanced) = partial.advanced {
        if let Some(value) = advanced.log_retention_days {
            base.advanced.log_retention_days = value;
        }
        if let Some(value) = advanced.enable_audit_logging {
            base.advanced.enable_audit_logging = value;
        }
        if let Some(value) = advanced.sidecar_port {
            base.advanced.sidecar_port = value;
        }
        if let Some(value) = advanced.allow_lan_mcp_access {
            base.advanced.allow_lan_mcp_access = value;
        }
        if let Some(value) = advanced.mcp_request_timeout_ms {
            base.advanced.mcp_request_timeout_ms = value;
        }
        if let Some(value) = advanced.mcp_server_start_timeout_ms {
            base.advanced.mcp_server_start_timeout_ms = value;
        }
        if let Some(value) = advanced.mcp_session_idle_ttl_ms {
            base.advanced.mcp_session_idle_ttl_ms = value;
        }
    }
    validate_settings(&base)?;
    Ok(base)
}

fn validate_settings(settings: &Settings) -> Result<(), String> {
    if settings.version == 0 {
        return Err("version must be at least 1".to_string());
    }
    if !matches!(
        settings.appearance.theme.as_str(),
        "light" | "dark" | "system"
    ) {
        return Err("appearance.theme must be light, dark, or system".to_string());
    }
    if settings.advanced.log_retention_days > 365 {
        return Err("advanced.logRetentionDays must be between 0 and 365".to_string());
    }
    if settings.advanced.sidecar_port < 1024 {
        return Err("advanced.sidecarPort must be between 1024 and 65535".to_string());
    }
    if settings.advanced.mcp_request_timeout_ms < MCP_TIMEOUT_MS_MIN
        || settings.advanced.mcp_request_timeout_ms > MCP_TIMEOUT_MS_MAX
    {
        return Err(format!(
            "advanced.mcpRequestTimeoutMs must be between {MCP_TIMEOUT_MS_MIN} and {MCP_TIMEOUT_MS_MAX}"
        ));
    }
    if settings.advanced.mcp_server_start_timeout_ms < MCP_TIMEOUT_MS_MIN
        || settings.advanced.mcp_server_start_timeout_ms > MCP_TIMEOUT_MS_MAX
    {
        return Err(format!(
            "advanced.mcpServerStartTimeoutMs must be between {MCP_TIMEOUT_MS_MIN} and {MCP_TIMEOUT_MS_MAX}"
        ));
    }
    if settings.advanced.mcp_session_idle_ttl_ms < MCP_SESSION_IDLE_TTL_MS_MIN
        || settings.advanced.mcp_session_idle_ttl_ms > MCP_SESSION_IDLE_TTL_MS_MAX
    {
        return Err(format!(
            "advanced.mcpSessionIdleTtlMs must be between {MCP_SESSION_IDLE_TTL_MS_MIN} and {MCP_SESSION_IDLE_TTL_MS_MAX}"
        ));
    }
    Ok(())
}

pub fn audit_logging_enabled(db: &Database) -> bool {
    get_settings(db)
        .map(|settings| settings.advanced.enable_audit_logging)
        .unwrap_or_else(|_| default_settings().advanced.enable_audit_logging)
}

#[cfg(test)]
mod tests {
    use super::*;
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
