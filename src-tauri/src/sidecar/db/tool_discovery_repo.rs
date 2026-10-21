use super::Database;
use crate::sidecar::services::tool_risk::{classify, RiskLevel};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiscovery {
    pub server_id: String,
    pub tool_name: String,
    pub exposed_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    /// `safe` / `write` / `destructive` / `unknown`
    pub risk_level: String,
    /// `heuristic`（推断）或 `user`（用户纠正）
    pub risk_source: String,
    pub discovered_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTool {
    pub server_id: String,
    pub server_name: String,
    /// 该 Server 在当前 Profile 内是否启用。查询会返回两种状态，可见性判定由调用方负责。
    pub enabled: bool,
    pub disabled_tools: Vec<String>,
    /// 该 Tool 是否在当前 Profile 的置顶集合里。
    pub pinned: bool,
    /// 启发式或用户纠正后的风险等级。
    pub risk_level: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

fn map_tool_discovery(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolDiscovery> {
    let input_schema_str: Option<String> = row.get("input_schema")?;
    Ok(ToolDiscovery {
        server_id: row.get("server_id")?,
        tool_name: row.get("tool_name")?,
        exposed_name: row.get("exposed_name")?,
        description: row.get("description")?,
        input_schema: input_schema_str.and_then(|s| serde_json::from_str(&s).ok()),
        risk_level: row.get("risk_level")?,
        risk_source: row.get("risk_source")?,
        discovered_at: row.get("discovered_at")?,
    })
}

pub struct ToolDiscoveryRepository<'a> {
    pub db: &'a Database,
}

impl<'a> ToolDiscoveryRepository<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn find_by_server_id(&self, server_id: &str) -> Result<Vec<ToolDiscovery>, String> {
        self.db.query_all(
            "SELECT * FROM tool_discoveries WHERE server_id = ?1",
            &[&server_id],
            map_tool_discovery,
        )
    }

    pub fn find_all(&self) -> Result<Vec<ToolDiscovery>, String> {
        self.db
            .query_all("SELECT * FROM tool_discoveries", &[], map_tool_discovery)
    }

    pub fn replace_tools_for_server(
        &self,
        server_id: &str,
        tools: &[ToolInsert],
    ) -> Result<(), String> {
        // 重新发现会整表替换；用户手动纠正过的风险等级必须先取出来再写回，
        // 否则每次刷新工具都会把纠正结果冲掉。
        let overrides: HashMap<String, String> = self
            .db
            .query_all(
                "SELECT tool_name, risk_level FROM tool_discoveries WHERE server_id = ?1 AND risk_source = 'user'",
                &[&server_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?
            .into_iter()
            .collect();
        self.db.transaction(|conn| {
            conn.execute(
                "DELETE FROM tool_discoveries WHERE server_id = ?1",
                [server_id],
            )
            .map_err(|e| e.to_string())?;
            let now = chrono::Utc::now().to_rfc3339();
            for tool in tools {
                let input_schema_json = tool
                    .input_schema
                    .as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
                let (risk_level, risk_source) = match overrides.get(&tool.name) {
                    Some(level) => (level.clone(), "user"),
                    None => (
                        classify(&tool.name, tool.description.as_deref())
                            .as_str()
                            .to_string(),
                        "heuristic",
                    ),
                };
                conn.execute(
                    "INSERT INTO tool_discoveries (server_id, tool_name, exposed_name, description, input_schema, risk_level, risk_source, discovered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        server_id,
                        tool.name,
                        tool.name,
                        tool.description,
                        input_schema_json,
                        risk_level,
                        risk_source,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    /// 用户手动纠正风险等级。之后重新发现工具也会保留这个来源。
    pub fn set_tool_risk(
        &self,
        server_id: &str,
        tool_name: &str,
        level: RiskLevel,
    ) -> Result<(), String> {
        let exists = self.db.query_one(
            "SELECT tool_name FROM tool_discoveries WHERE server_id = ?1 AND tool_name = ?2",
            &[&server_id, &tool_name],
            |row| row.get::<_, String>(0),
        )?;
        if exists.is_none() {
            return Err(format!("Tool \"{tool_name}\" not found"));
        }
        self.db.run(
            "UPDATE tool_discoveries SET risk_level = ?1, risk_source = 'user' WHERE server_id = ?2 AND tool_name = ?3",
            &[&level.as_str(), &server_id, &tool_name],
        )
    }

    pub fn find_by_profile_id(&self, profile_id: &str) -> Result<Vec<ProfileTool>, String> {
        self.db.query_all(
            "SELECT ps.server_id, ps.enabled, ms.name AS server_name, ps.disabled_tools, ps.pinned_tools, td.tool_name, td.description, td.input_schema, td.risk_level
             FROM profile_servers ps
             JOIN mcp_servers ms ON ps.server_id = ms.id
             JOIN tool_discoveries td ON td.server_id = ms.id
             WHERE ps.profile_id = ?1
             ORDER BY ms.name ASC, td.tool_name ASC",
            &[&profile_id],
            |row| {
                let disabled_tools_str: String = row.get("disabled_tools")?;
                let pinned_tools_str: String = row.get("pinned_tools")?;
                let input_schema_str: Option<String> = row.get("input_schema")?;
                let tool_name: String = row.get("tool_name")?;
                let risk_level: String = row.get("risk_level")?;
                let pinned_tools: Vec<String> =
                    serde_json::from_str(&pinned_tools_str).unwrap_or_default();
                Ok(ProfileTool {
                    server_id: row.get("server_id")?,
                    server_name: row.get("server_name")?,
                    enabled: row.get::<_, i64>("enabled")? != 0,
                    disabled_tools: serde_json::from_str(&disabled_tools_str).unwrap_or_default(),
                    pinned: pinned_tools.contains(&tool_name),
                    risk_level,
                    tool_name,
                    description: row.get("description")?,
                    input_schema: input_schema_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            },
        )
    }

    pub fn delete_by_server_id(&self, server_id: &str) -> Result<(), String> {
        self.db.run(
            "DELETE FROM tool_discoveries WHERE server_id = ?1",
            &[&server_id],
        )
    }
}

pub struct ToolInsert {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_db(test_name: &str) -> Database {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "moor-tool-discovery-{test_name}-{ts}-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::open(&path).expect("open db");
        db.run_migrations().expect("migrate");
        db
    }

    fn insert_server(db: &Database) {
        db.run(
            "INSERT INTO mcp_servers (id, name, connection_type, status, created_at, updated_at)
             VALUES ('server-1', 'Alpha', 'stdio', 'stopped', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            &[],
        )
        .expect("insert server");
    }

    fn tools() -> Vec<ToolInsert> {
        vec![ToolInsert {
            name: "delete_repo".to_string(),
            description: None,
            input_schema: None,
        }]
    }

    #[test]
    fn discovery_classifies_tool_risk() {
        let db = temp_db("risk-classify");
        insert_server(&db);
        let repo = ToolDiscoveryRepository::new(&db);
        repo.replace_tools_for_server("server-1", &tools())
            .expect("discover");

        let discovered = repo.find_by_server_id("server-1").expect("read tools");
        assert_eq!(discovered[0].risk_level, "destructive");
        assert_eq!(discovered[0].risk_source, "heuristic");
    }

    #[test]
    fn a_user_override_survives_rediscovery() {
        let db = temp_db("risk-override");
        insert_server(&db);
        let repo = ToolDiscoveryRepository::new(&db);
        repo.replace_tools_for_server("server-1", &tools())
            .expect("discover");
        repo.set_tool_risk("server-1", "delete_repo", RiskLevel::Safe)
            .expect("override");

        repo.replace_tools_for_server("server-1", &tools())
            .expect("rediscover");

        let discovered = repo.find_by_server_id("server-1").expect("read tools");
        assert_eq!(discovered[0].risk_level, "safe");
        assert_eq!(discovered[0].risk_source, "user");
    }
}
