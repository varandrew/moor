use super::Database;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    /// `direct`（全量直出）或 `curated`（置顶直出）。老库升级后恒为 `direct`。
    pub exposure_mode: String,
    /// `advisory`（只标记）或 `block_destructive`（拒绝破坏性工具）。
    pub risk_policy: String,
    pub server_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileServerState {
    pub enabled: bool,
    pub disabled_tools: Vec<String>,
    pub pinned_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetailServer {
    #[serde(flatten)]
    pub server: super::server_repo::Server,
    pub profile_server: ProfileServerState,
}

/// Profile 的可选更新补丁：每一项缺省都表示「保持不变」。
#[derive(Debug, Clone, Default)]
pub struct ProfileUpdate<'a> {
    pub name: Option<&'a str>,
    pub exposure_mode: Option<&'a str>,
    pub risk_policy: Option<&'a str>,
    pub initial_pins: Option<&'a [InitialPin]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialPin {
    pub server_id: String,
    pub tool_name: String,
}

pub enum ProfileUpdateResult {
    Updated(Profile),
    NotFound,
    ServerNotInProfile(String),
}

/// 批量 upsert 的单项输入：enabled 与 disabled_tools 均为可选补丁，语义与单条接口一致。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileServerUpsertInput {
    pub server_id: String,
    pub enabled: Option<bool>,
    pub disabled_tools: Option<Vec<String>>,
    pub pinned_tools: Option<Vec<String>>,
}

/// upsert 合并语义的唯一实现：enabled 与 disabled_tools 是相互独立的可选补丁，
/// 缺省方保留现有值；新行缺省 enabled=true / 空禁用表。
/// 单条与批量接口共用——两处语义分叉会导致治理面板与单点编辑互相覆盖。
fn resolve_profile_server_state(
    existing: Option<(i64, String, String)>,
    enabled: Option<bool>,
    disabled_tools: Option<&Vec<String>>,
    pinned_tools: Option<&Vec<String>>,
) -> (i64, String, String) {
    let tools_json =
        |tools: &Vec<String>| serde_json::to_string(tools).unwrap_or_else(|_| "[]".into());
    let (current_enabled, current_disabled, current_pinned) = match existing {
        Some((enabled, disabled, pinned)) => (enabled, disabled, pinned),
        None => (1, "[]".to_string(), "[]".to_string()),
    };
    (
        enabled.map_or(current_enabled, |value| value as i64),
        disabled_tools.map_or(current_disabled, |tools| tools_json(tools)),
        pinned_tools.map_or(current_pinned, |tools| tools_json(tools)),
    )
}

fn map_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    let is_active: i64 = row.get("is_active")?;
    let server_count: Option<i64> = row.get("server_count")?;
    Ok(Profile {
        id: row.get("id")?,
        name: row.get("name")?,
        is_active: is_active != 0,
        exposure_mode: row.get("exposure_mode")?,
        risk_policy: row.get("risk_policy")?,
        server_count,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn write_profile_server(
    conn: &Connection,
    profile_id: &str,
    item: &ProfileServerUpsertInput,
) -> Result<(), String> {
    let existing = conn.query_row(
        "SELECT enabled, disabled_tools, pinned_tools FROM profile_servers WHERE profile_id = ?1 AND server_id = ?2",
        rusqlite::params![profile_id, item.server_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
    ).optional().map_err(|e| e.to_string())?;
    let (enabled, disabled, pinned) = resolve_profile_server_state(
        existing,
        item.enabled,
        item.disabled_tools.as_ref(),
        item.pinned_tools.as_ref(),
    );
    conn.execute(
        "INSERT INTO profile_servers (profile_id, server_id, enabled, disabled_tools, pinned_tools) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(profile_id, server_id) DO UPDATE SET enabled = excluded.enabled, disabled_tools = excluded.disabled_tools, pinned_tools = excluded.pinned_tools",
        rusqlite::params![profile_id, item.server_id, enabled, disabled, pinned],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub struct ProfileRepository<'a> {
    pub db: &'a Database,
}

impl<'a> ProfileRepository<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn find_all(&self) -> Result<Vec<Profile>, String> {
        self.db.query_all(
            "SELECT p.*, COUNT(ps.server_id) as server_count
             FROM profiles p
             LEFT JOIN profile_servers ps ON p.id = ps.profile_id
             GROUP BY p.id
             ORDER BY p.created_at DESC",
            &[],
            map_profile,
        )
    }

    pub fn find_by_id(&self, id: &str) -> Result<Option<Profile>, String> {
        self.db.query_one(
            "SELECT p.*, COUNT(ps.server_id) as server_count
             FROM profiles p
             LEFT JOIN profile_servers ps ON p.id = ps.profile_id
             WHERE p.id = ?1
             GROUP BY p.id",
            &[&id],
            map_profile,
        )
    }

    pub fn find_active_id(&self) -> Result<Option<String>, String> {
        self.db
            .query_one("SELECT id FROM profiles WHERE is_active = 1", &[], |row| {
                row.get(0)
            })
    }

    pub fn create(&self, name: &str) -> Result<Profile, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.db.run(
            "INSERT INTO profiles (id, name, is_active, created_at, updated_at) VALUES (?1, ?2, 0, ?3, ?4)",
            &[&id, &name, &now, &now],
        )?;
        self.find_by_id(&id)
            .and_then(|p| p.ok_or_else(|| "Created profile could not be reloaded".into()))
    }

    pub fn update(
        &self,
        id: &str,
        update: ProfileUpdate<'_>,
    ) -> Result<ProfileUpdateResult, String> {
        self.db.transaction(|conn| {
            let exists = conn.query_row("SELECT id FROM profiles WHERE id = ?1", [id], |row| row.get::<_, String>(0))
                .optional().map_err(|e| e.to_string())?;
            if exists.is_none() {
                return Ok(ProfileUpdateResult::NotFound);
            }
            let mut pins_by_server = std::collections::BTreeMap::<String, Vec<String>>::new();
            for pin in update.initial_pins.unwrap_or_default() {
                if !pins_by_server.contains_key(&pin.server_id) {
                    let stored = conn.query_row(
                        "SELECT pinned_tools FROM profile_servers WHERE profile_id = ?1 AND server_id = ?2",
                        rusqlite::params![id, pin.server_id], |row| row.get::<_, String>(0),
                    ).optional().map_err(|e| e.to_string())?;
                    let Some(stored) = stored else {
                        return Ok(ProfileUpdateResult::ServerNotInProfile(pin.server_id.clone()));
                    };
                    pins_by_server.insert(pin.server_id.clone(), serde_json::from_str(&stored).map_err(|e| e.to_string())?);
                }
                let pins = pins_by_server.get_mut(&pin.server_id).expect("server pins loaded");
                if !pins.contains(&pin.tool_name) {
                    pins.push(pin.tool_name.clone());
                }
            }
            for (server_id, pinned_tools) in pins_by_server {
                write_profile_server(conn, id, &ProfileServerUpsertInput {
                    server_id, enabled: None, disabled_tools: None, pinned_tools: Some(pinned_tools),
                })?;
            }
            if update.name.is_some() || update.exposure_mode.is_some() || update.risk_policy.is_some() {
                conn.execute(
                    "UPDATE profiles SET name = COALESCE(?1, name), exposure_mode = COALESCE(?2, exposure_mode), risk_policy = COALESCE(?3, risk_policy), updated_at = ?4 WHERE id = ?5",
                    rusqlite::params![update.name, update.exposure_mode, update.risk_policy, chrono::Utc::now().to_rfc3339(), id],
                ).map_err(|e| e.to_string())?;
            }
            conn.query_row(
                "SELECT p.*, COUNT(ps.server_id) as server_count FROM profiles p LEFT JOIN profile_servers ps ON p.id = ps.profile_id WHERE p.id = ?1 GROUP BY p.id",
                [id], map_profile,
            ).map(ProfileUpdateResult::Updated).map_err(|e| e.to_string())
        })
    }

    pub fn activate(&self, id: &str) -> Result<Option<Profile>, String> {
        let exists = self
            .db
            .query_one("SELECT id FROM profiles WHERE id = ?1", &[&id], |row| {
                row.get::<_, String>(0)
            })?;
        if exists.is_none() {
            return Ok(None);
        }
        self.db.transaction(|conn| {
            conn.execute("UPDATE profiles SET is_active = 0 WHERE id != ?1", [id])
                .map_err(|e| e.to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE profiles SET is_active = 1, updated_at = ?1 WHERE id = ?2",
                rusqlite::params![&now, &id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        self.find_by_id(id)
    }

    pub fn remove(&self, id: &str) -> Result<RemoveResult, String> {
        let existing = self.db.query_one(
            "SELECT id, is_active FROM profiles WHERE id = ?1",
            &[&id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        match existing {
            None => Ok(RemoveResult::NotFound),
            Some((_, is_active)) if is_active != 0 => Ok(RemoveResult::Active),
            Some(_) => {
                self.db.run("DELETE FROM profiles WHERE id = ?1", &[&id])?;
                Ok(RemoveResult::Success)
            }
        }
    }

    pub fn find_profile_servers(
        &self,
        profile_id: &str,
    ) -> Result<Vec<ProfileDetailServer>, String> {
        self.db.query_all(
            "SELECT ms.*, ps.enabled AS profile_enabled, ps.disabled_tools AS profile_disabled_tools,
                    ps.pinned_tools AS profile_pinned_tools
             FROM mcp_servers ms
             LEFT JOIN profile_servers ps ON ps.server_id = ms.id AND ps.profile_id = ?1
             ORDER BY ms.name ASC",
            &[&profile_id],
            |row| {
                let server = super::server_repo::map_server(row)?;
                let profile_enabled: Option<i64> = row.get("profile_enabled")?;
                let profile_disabled_tools: Option<String> = row.get("profile_disabled_tools")?;
                let profile_pinned_tools: Option<String> = row.get("profile_pinned_tools")?;
                Ok(ProfileDetailServer {
                    server,
                    profile_server: ProfileServerState {
                        enabled: profile_enabled.is_some_and(|v| v != 0),
                        disabled_tools: profile_disabled_tools
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default(),
                        pinned_tools: profile_pinned_tools
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default(),
                    },
                })
            },
        )
    }

    pub fn upsert_profile_server(
        &self,
        profile_id: &str,
        server_id: &str,
        enabled: Option<bool>,
        disabled_tools: Option<&Vec<String>>,
    ) -> Result<ProfileServerState, String> {
        let existing = self.db.query_one(
            "SELECT enabled, disabled_tools, pinned_tools FROM profile_servers WHERE profile_id = ?1 AND server_id = ?2",
            &[&profile_id, &server_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;

        let had_existing = existing.is_some();
        let (final_enabled, final_tools, final_pinned) =
            resolve_profile_server_state(existing, enabled, disabled_tools, None);

        if had_existing {
            self.db.run(
                "UPDATE profile_servers SET enabled = ?1, disabled_tools = ?2 WHERE profile_id = ?3 AND server_id = ?4",
                &[&final_enabled, &final_tools, &profile_id, &server_id],
            )?;
        } else {
            self.db.run(
                "INSERT INTO profile_servers (profile_id, server_id, enabled, disabled_tools) VALUES (?1, ?2, ?3, ?4)",
                &[&profile_id, &server_id, &final_enabled, &final_tools],
            )?;
        }

        Ok(ProfileServerState {
            enabled: final_enabled != 0,
            disabled_tools: serde_json::from_str(&final_tools).unwrap_or_default(),
            pinned_tools: serde_json::from_str(&final_pinned).unwrap_or_default(),
        })
    }

    /// 批量 upsert：单事务内逐项应用与单条接口相同的合并语义。
    /// 治理面板的批量开关依赖原子性——部分成功会让 UI 与网关目录状态分叉。
    pub fn upsert_profile_servers_batch(
        &self,
        profile_id: &str,
        items: &[ProfileServerUpsertInput],
    ) -> Result<(), String> {
        self.db.transaction(|conn| {
            for item in items {
                write_profile_server(conn, profile_id, item)?;
            }
            Ok(())
        })
    }

    /// 克隆 profile：复制全部 profile_servers（enabled + disabled_tools），新 profile 恒为非激活。
    /// 「从现有 profile 创建」入口的唯一数据路径。不存在时返回 None。
    pub fn clone_profile(
        &self,
        source_id: &str,
        new_name: &str,
    ) -> Result<Option<Profile>, String> {
        let source = self.db.query_one(
            "SELECT id FROM profiles WHERE id = ?1",
            &[&source_id],
            |row| row.get::<_, String>(0),
        )?;
        if source.is_none() {
            return Ok(None);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.db.transaction(|conn| {
            conn.execute(
                "INSERT INTO profiles (id, name, is_active, exposure_mode, risk_policy, created_at, updated_at)
                 SELECT ?1, ?2, 0, exposure_mode, risk_policy, ?3, ?3 FROM profiles WHERE id = ?4",
                rusqlite::params![&id, &new_name, &now, &source_id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO profile_servers (profile_id, server_id, enabled, disabled_tools, pinned_tools)
                 SELECT ?1, server_id, enabled, disabled_tools, pinned_tools FROM profile_servers WHERE profile_id = ?2",
                rusqlite::params![&id, &source_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        self.find_by_id(&id)
    }

    pub fn find_active_profile_server_ids(&self) -> Result<Vec<String>, String> {
        let active_id = self.find_active_id()?;
        let Some(active_id) = active_id else {
            return Ok(vec![]);
        };
        self.db.query_all(
            "SELECT server_id FROM profile_servers WHERE profile_id = ?1 AND enabled = 1",
            &[&active_id],
            |row| row.get(0),
        )
    }

    #[cfg(test)]
    pub fn assign_to_active_profile(&self, server_ids: &[String]) -> Result<(), String> {
        let active =
            self.db
                .query_one("SELECT id FROM profiles WHERE is_active = 1", &[], |row| {
                    row.get::<_, String>(0)
                })?;
        let Some(active_id) = active else {
            return Ok(());
        };
        for server_id in server_ids {
            self.db.run(
                "INSERT OR IGNORE INTO profile_servers (profile_id, server_id, enabled, disabled_tools) VALUES (?1, ?2, 1, '[]')",
                &[&active_id, &server_id],
            )?;
        }
        Ok(())
    }

    pub fn seed_default(&self) -> Result<(), String> {
        let existing_profiles =
            self.db
                .query_all("SELECT id, is_active FROM profiles", &[], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?;
        if existing_profiles
            .iter()
            .any(|(_, is_active)| *is_active != 0)
        {
            return Ok(());
        }

        let rows = self.db.query_all(
            "SELECT id FROM profiles WHERE name = 'Default'",
            &[],
            |row| row.get::<_, String>(0),
        )?;
        let now = chrono::Utc::now().to_rfc3339();
        if rows.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            self.db.run(
                "INSERT INTO profiles (id, name, is_active, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?4)",
                &[&id, &"Default", &now, &now],
            )?;
        } else {
            self.db.run(
                "UPDATE profiles SET is_active = 1, updated_at = ?1 WHERE id = ?2",
                &[&now, &rows[0]],
            )?;
        }
        Ok(())
    }
}

pub enum RemoveResult {
    Success,
    NotFound,
    Active,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::Database;
    use std::time::SystemTime;

    fn temp_db_path(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("moor-profile-{test_name}-{timestamp}"))
            .join("moor.db")
    }

    #[test]
    fn seed_default_preserves_existing_active_profile() {
        let db_path = temp_db_path("preserve-active");
        std::fs::create_dir_all(db_path.parent().unwrap()).expect("failed to create temp db dir");
        let db = Database::open(&db_path).expect("failed to open temp db");
        db.run_migrations().expect("failed to migrate temp db");
        let repo = ProfileRepository::new(&db);
        let first = repo.create("Work").expect("failed to create profile");
        repo.activate(&first.id)
            .expect("failed to activate profile");

        repo.seed_default().expect("failed to seed default");

        assert_eq!(repo.find_active_id().unwrap(), Some(first.id));
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }
}
