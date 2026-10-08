use super::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiscovery {
    pub server_id: String,
    pub tool_name: String,
    pub exposed_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub discovered_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTool {
    pub server_id: String,
    pub server_name: String,
    pub disabled_tools: Vec<String>,
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
                conn.execute(
                    "INSERT INTO tool_discoveries (server_id, tool_name, exposed_name, description, input_schema, discovered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        server_id,
                        tool.name,
                        tool.name,
                        tool.description,
                        input_schema_json,
                        &now,
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    pub fn find_by_profile_id(&self, profile_id: &str) -> Result<Vec<ProfileTool>, String> {
        self.db.query_all(
            "SELECT ps.server_id, ms.name AS server_name, ps.disabled_tools, td.tool_name, td.description, td.input_schema
             FROM profile_servers ps
             JOIN mcp_servers ms ON ps.server_id = ms.id
             JOIN tool_discoveries td ON td.server_id = ms.id
             WHERE ps.profile_id = ?1 AND ps.enabled = 1
             ORDER BY ms.name ASC, td.tool_name ASC",
            &[&profile_id],
            |row| {
                let disabled_tools_str: String = row.get("disabled_tools")?;
                let input_schema_str: Option<String> = row.get("input_schema")?;
                Ok(ProfileTool {
                    server_id: row.get("server_id")?,
                    server_name: row.get("server_name")?,
                    disabled_tools: serde_json::from_str(&disabled_tools_str).unwrap_or_default(),
                    tool_name: row.get("tool_name")?,
                    description: row.get("description")?,
                    input_schema: input_schema_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            },
        )
    }

    pub fn find_disabled_tools_for_server(
        &self,
        profile_id: Option<&str>,
        server_id: &str,
    ) -> Result<std::collections::HashSet<String>, String> {
        let rows = match profile_id {
            Some(pid) => self.db.query_all(
                "SELECT disabled_tools FROM profile_servers WHERE profile_id = ?1 AND server_id = ?2",
                &[&pid, &server_id],
                |row| row.get::<_, String>(0),
            )?,
            None => self.db.query_all(
                "SELECT disabled_tools FROM profile_servers WHERE server_id = ?1",
                &[&server_id],
                |row| row.get::<_, String>(0),
            )?,
        };
        Ok(rows
            .iter()
            .flat_map(|s| serde_json::from_str::<Vec<String>>(s).unwrap_or_default())
            .collect())
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
