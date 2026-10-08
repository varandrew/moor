use super::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: String,
    pub profile_id: Option<String>,
    pub server_id: Option<String>,
    pub tool_name: String,
    pub arguments: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub agent_info: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStats {
    pub total_calls: i64,
    pub error_calls: i64,
    pub error_rate: f64,
    pub avg_duration_ms: Option<f64>,
    pub top_tools: Vec<TopTool>,
    pub top_servers: Vec<TopServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopTool {
    pub tool_name: String,
    pub count: i64,
    pub avg_duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopServer {
    pub server_id: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInsight {
    pub tool_name: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub call_count: i64,
    pub error_count: i64,
    pub error_rate: f64,
    pub avg_duration_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub last_called_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInsight {
    pub server_id: String,
    pub server_name: Option<String>,
    pub call_count: i64,
    pub error_count: i64,
    pub error_rate: f64,
    pub avg_duration_ms: Option<f64>,
    pub last_called_at: Option<String>,
}

/// 调用洞察：在时间窗口内按 (server, tool) 与 server 两级聚合。
/// 与 LogStats(全局、无窗口)并行存在，洞察面板是它的新消费方。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogInsights {
    pub total_calls: i64,
    pub error_calls: i64,
    pub error_rate: f64,
    pub avg_duration_ms: Option<f64>,
    pub tools: Vec<ToolInsight>,
    pub servers: Vec<ServerInsight>,
}

fn map_audit_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditLogEntry> {
    let args_str: Option<String> = row.get("arguments")?;
    let result_str: Option<String> = row.get("result")?;
    Ok(AuditLogEntry {
        id: row.get("id")?,
        timestamp: row.get("timestamp")?,
        profile_id: row.get("profile_id")?,
        server_id: row.get("server_id")?,
        tool_name: row.get("tool_name")?,
        arguments: args_str.and_then(|s| serde_json::from_str(&s).ok()),
        result: result_str.and_then(|s| serde_json::from_str(&s).ok()),
        error: row.get("error")?,
        duration_ms: row.get("duration_ms")?,
        agent_info: row.get("agent_info")?,
    })
}

/// 已排序数据的 nearest-rank 分位数：rank = ceil(q/100 * n)，取第 rank 个值（1-based）。
fn percentile_nearest_rank(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let rank = ((q / 100.0) * values.len() as f64).ceil() as usize;
    let idx = rank.clamp(1, values.len()) - 1;
    Some(values[idx])
}

pub struct AuditLogRepository<'a> {
    pub db: &'a Database,
}

impl<'a> AuditLogRepository<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn query_logs(
        &self,
        server_id: Option<&str>,
        tool_name: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<AuditLogEntry>, String> {
        let mut sql = "SELECT * FROM audit_logs WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(sid) = server_id {
            sql += " AND server_id = ?";
            params.push(Box::new(sid.to_string()));
        }
        if let Some(tn) = tool_name {
            sql += " AND tool_name = ?";
            params.push(Box::new(tn.to_string()));
        }
        if let Some(f) = from {
            sql += " AND timestamp >= ?";
            params.push(Box::new(f.to_string()));
        }
        if let Some(t) = to {
            sql += " AND timestamp <= ?";
            params.push(Box::new(t.to_string()));
        }

        let safe_limit = limit.map_or(50, |l| l.clamp(1, 200));
        let safe_offset = offset.unwrap_or(0);
        sql += " ORDER BY timestamp DESC LIMIT ? OFFSET ?";
        params.push(Box::new(safe_limit));
        params.push(Box::new(safe_offset));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        self.db.query_all(&sql, &param_refs, map_audit_log)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &self,
        id: &str,
        timestamp: &str,
        profile_id: Option<&str>,
        server_id: Option<&str>,
        tool_name: &str,
        arguments: Option<&serde_json::Value>,
        result: Option<&serde_json::Value>,
        error: Option<&str>,
        duration_ms: i64,
        agent_info: Option<&str>,
    ) -> Result<(), String> {
        let args_json = arguments.map(|v| serde_json::to_string(v).unwrap_or_default());
        let result_json = result.map(|v| serde_json::to_string(v).unwrap_or_default());
        self.db.run(
            "INSERT INTO audit_logs (id, timestamp, profile_id, server_id, tool_name, arguments, result, error, duration_ms, agent_info)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            &[&id, &timestamp, &profile_id, &server_id, &tool_name, &args_json, &result_json, &error, &duration_ms, &agent_info],
        )
    }

    /// 按时间窗口聚合洞察。分位数用 nearest-rank：本地单用户量级下
    /// 直接拉窗口内 duration 明细在内存排序，量级受 retention 上限约束。
    pub fn get_insights(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<LogInsights, String> {
        let mut conditions = String::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(f) = from {
            conditions += " AND a.timestamp >= ?";
            params.push(Box::new(f.to_string()));
        }
        if let Some(t) = to {
            conditions += " AND a.timestamp <= ?";
            params.push(Box::new(t.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let (total_calls, error_calls, avg_duration_ms) = self
            .db
            .query_one(
                &format!(
                    "SELECT COUNT(*), COALESCE(SUM(CASE WHEN a.error IS NOT NULL THEN 1 ELSE 0 END), 0), AVG(a.duration_ms)
                     FROM audit_logs a WHERE 1=1{conditions}"
                ),
                &param_refs,
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                    ))
                },
            )?
            .unwrap_or((0, 0, None));
        let error_rate = if total_calls > 0 {
            error_calls as f64 / total_calls as f64
        } else {
            0.0
        };

        let mut durations: std::collections::HashMap<(Option<String>, String), Vec<f64>> =
            std::collections::HashMap::new();
        self.db.query_all(
            &format!(
                "SELECT a.server_id, a.tool_name, a.duration_ms FROM audit_logs a
                 WHERE a.duration_ms IS NOT NULL{conditions}"
            ),
            &param_refs,
            |row| {
                let key = (row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?);
                durations
                    .entry(key)
                    .or_default()
                    .push(row.get::<_, f64>(2)?);
                Ok(())
            },
        )?;

        let mut tools = self.db.query_all(
            &format!(
                "SELECT a.tool_name, a.server_id, s.name AS server_name, COUNT(*) AS call_count,
                        COALESCE(SUM(CASE WHEN a.error IS NOT NULL THEN 1 ELSE 0 END), 0) AS error_count,
                        AVG(a.duration_ms) AS avg_duration_ms, MAX(a.timestamp) AS last_called_at
                 FROM audit_logs a LEFT JOIN mcp_servers s ON s.id = a.server_id
                 WHERE 1=1{conditions}
                 GROUP BY a.server_id, a.tool_name
                 ORDER BY call_count DESC"
            ),
            &param_refs,
            |row| {
                let call_count: i64 = row.get(3)?;
                let error_count: i64 = row.get(4)?;
                Ok(ToolInsight {
                    tool_name: row.get(0)?,
                    server_id: row.get(1)?,
                    server_name: row.get(2)?,
                    call_count,
                    error_count,
                    error_rate: if call_count > 0 {
                        error_count as f64 / call_count as f64
                    } else {
                        0.0
                    },
                    avg_duration_ms: row.get(5)?,
                    p50_ms: None,
                    p95_ms: None,
                    last_called_at: row.get(6)?,
                })
            },
        )?;
        for tool in &mut tools {
            if let Some(vals) = durations.get_mut(&(tool.server_id.clone(), tool.tool_name.clone())) {
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                tool.p50_ms = percentile_nearest_rank(vals, 50.0);
                tool.p95_ms = percentile_nearest_rank(vals, 95.0);
            }
        }

        let servers = self.db.query_all(
            &format!(
                "SELECT a.server_id, s.name AS server_name, COUNT(*) AS call_count,
                        COALESCE(SUM(CASE WHEN a.error IS NOT NULL THEN 1 ELSE 0 END), 0) AS error_count,
                        AVG(a.duration_ms) AS avg_duration_ms, MAX(a.timestamp) AS last_called_at
                 FROM audit_logs a LEFT JOIN mcp_servers s ON s.id = a.server_id
                 WHERE a.server_id IS NOT NULL{conditions}
                 GROUP BY a.server_id
                 ORDER BY call_count DESC"
            ),
            &param_refs,
            |row| {
                let call_count: i64 = row.get(2)?;
                let error_count: i64 = row.get(3)?;
                Ok(ServerInsight {
                    server_id: row.get(0)?,
                    server_name: row.get(1)?,
                    call_count,
                    error_count,
                    error_rate: if call_count > 0 {
                        error_count as f64 / call_count as f64
                    } else {
                        0.0
                    },
                    avg_duration_ms: row.get(4)?,
                    last_called_at: row.get(5)?,
                })
            },
        )?;

        Ok(LogInsights {
            total_calls,
            error_calls,
            error_rate,
            avg_duration_ms,
            tools,
            servers,
        })
    }

    /// 滚动保留：删除 timestamp 早于 cutoff 的审计行，返回删除行数。
    /// 事务内执行以拿到准确的 affected count。
    pub fn purge_before(&self, cutoff: &str) -> Result<usize, String> {
        self.db.transaction(|conn| {
            conn.execute("DELETE FROM audit_logs WHERE timestamp < ?1", [cutoff])
                .map_err(|e| e.to_string())
        })
    }

    pub fn get_stats(&self) -> Result<LogStats, String> {
        let summary = self.db.query_one(
            "SELECT COUNT(*) as total_calls, COALESCE(SUM(CASE WHEN error IS NOT NULL THEN 1 ELSE 0 END), 0) as error_calls, AVG(duration_ms) as avg_duration_ms FROM audit_logs",
            &[],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                ))
            },
        )?;

        let (total_calls, error_calls, avg_duration_ms) = summary.unwrap_or((0, 0, None));
        let error_rate = if total_calls > 0 {
            error_calls as f64 / total_calls as f64
        } else {
            0.0
        };

        let top_tools = self.db.query_all(
            "SELECT tool_name, COUNT(*) as count, AVG(duration_ms) as avg_duration FROM audit_logs GROUP BY tool_name ORDER BY count DESC LIMIT 10",
            &[],
            |row| {
                Ok(TopTool {
                    tool_name: row.get(0)?,
                    count: row.get(1)?,
                    avg_duration: row.get(2)?,
                })
            },
        )?;

        let top_servers = self.db.query_all(
            "SELECT server_id, COUNT(*) as count FROM audit_logs WHERE server_id IS NOT NULL GROUP BY server_id ORDER BY count DESC LIMIT 10",
            &[],
            |row| {
                Ok(TopServer {
                    server_id: row.get(0)?,
                    count: row.get(1)?,
                })
            },
        )?;

        Ok(LogStats {
            total_calls,
            error_calls,
            error_rate,
            avg_duration_ms,
            top_tools,
            top_servers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_db() -> (Database, std::path::PathBuf) {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moor-audit-repo-{ts}.db"));
        let db = Database::open(&path).expect("open db");
        db.run_migrations().expect("migrate");
        (db, path)
    }

    fn insert_log(
        repo: &AuditLogRepository,
        id: &str,
        tool: &str,
        server: Option<&str>,
        error: Option<&str>,
        duration_ms: i64,
    ) {
        repo.insert(
            id,
            "2026-01-01T00:00:00Z",
            None,
            server,
            tool,
            None,
            None,
            error,
            duration_ms,
            None,
        )
        .expect("insert log");
    }

    fn insert_server(db: &Database, id: &str) {
        use super::super::server_repo::{ServerInsertInput, ServerRepository};
        ServerRepository::new(db)
            .insert_one_with_id(
                id,
                0,
                &ServerInsertInput {
                    name: id.into(),
                    connection_type: "stdio".into(),
                    command: Some("node".into()),
                    args: None,
                    url: None,
                    env: None,
                    headers: None,
                    working_dir: None,
                    auto_start: false,
                },
            )
            .expect("insert server");
    }

    #[test]
    fn stats_counts_errors_and_computes_error_rate() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        insert_server(&db, "s1");
        insert_log(&repo, "a", "search", Some("s1"), None, 10);
        insert_log(&repo, "b", "search", Some("s1"), None, 20);
        insert_log(&repo, "c", "fetch", None, Some("boom"), 30);

        let stats = repo.get_stats().expect("stats");

        assert_eq!(stats.total_calls, 3);
        assert_eq!(stats.error_calls, 1);
        assert!((stats.error_rate - 1.0 / 3.0).abs() < 1e-9);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stats_average_duration_over_logged_calls() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        insert_log(&repo, "a", "search", None, None, 10);
        insert_log(&repo, "b", "search", None, None, 20);

        let stats = repo.get_stats().expect("stats");

        assert_eq!(stats.avg_duration_ms, Some(15.0));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stats_ranks_top_tools_and_skips_null_servers() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        insert_server(&db, "s1");
        insert_log(&repo, "a", "search", Some("s1"), None, 10);
        insert_log(&repo, "b", "search", Some("s1"), None, 20);
        insert_log(&repo, "c", "fetch", None, None, 30);

        let stats = repo.get_stats().expect("stats");

        assert_eq!(stats.top_tools.len(), 2);
        assert_eq!(stats.top_tools[0].tool_name, "search");
        assert_eq!(stats.top_tools[0].count, 2);
        assert_eq!(stats.top_servers.len(), 1);
        assert_eq!(stats.top_servers[0].server_id, "s1");
        assert_eq!(stats.top_servers[0].count, 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn insights_groups_by_server_and_tool_with_error_rate() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        insert_server(&db, "s1");
        insert_log(&repo, "a", "search", Some("s1"), None, 10);
        insert_log(&repo, "b", "search", Some("s1"), Some("boom"), 20);
        insert_log(&repo, "c", "fetch", None, None, 30);

        let insights = repo.get_insights(None, None).expect("insights");

        assert_eq!(insights.total_calls, 3);
        assert_eq!(insights.error_calls, 1);
        // 同名工具在不同 server 上必须分开统计，才能支撑「跳到该 server 的工具开关」
        assert_eq!(insights.tools.len(), 2);
        let search = insights
            .tools
            .iter()
            .find(|t| t.tool_name == "search")
            .expect("search row");
        assert_eq!(search.server_id.as_deref(), Some("s1"));
        assert_eq!(search.server_name.as_deref(), Some("s1"));
        assert_eq!(search.call_count, 2);
        assert_eq!(search.error_count, 1);
        assert!((search.error_rate - 0.5).abs() < 1e-9);
        assert_eq!(insights.servers.len(), 1);
        assert_eq!(insights.servers[0].call_count, 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn insights_respects_time_window() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        repo.insert(
            "old",
            "2026-01-01T00:00:00Z",
            None,
            None,
            "search",
            None,
            None,
            None,
            5,
            None,
        )
        .expect("insert");
        repo.insert(
            "new",
            "2026-02-01T00:00:00Z",
            None,
            None,
            "fetch",
            None,
            None,
            None,
            9,
            None,
        )
        .expect("insert");

        let insights = repo
            .get_insights(Some("2026-01-15T00:00:00Z"), None)
            .expect("insights");

        assert_eq!(insights.total_calls, 1);
        assert_eq!(insights.tools.len(), 1);
        assert_eq!(insights.tools[0].tool_name, "fetch");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn insights_percentiles_use_nearest_rank() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        for (i, duration) in [10, 20, 30, 40, 100].iter().enumerate() {
            repo.insert(
                &format!("l{i}"),
                "2026-01-01T00:00:00Z",
                None,
                None,
                "search",
                None,
                None,
                None,
                *duration,
                None,
            )
            .expect("insert");
        }

        let insights = repo.get_insights(None, None).expect("insights");
        let tool = &insights.tools[0];

        // n=5: p50 rank=ceil(2.5)=3 → 30; p95 rank=ceil(4.75)=5 → 100
        assert_eq!(tool.p50_ms, Some(30.0));
        assert_eq!(tool.p95_ms, Some(100.0));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn insights_percentiles_sort_unordered_calls() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        for (index, duration) in [100, 10, 40, 20, 30].iter().enumerate() {
            insert_log(
                &repo,
                &format!("unordered-{index}"),
                "search",
                None,
                None,
                *duration,
            );
        }

        let insights = repo.get_insights(None, None).expect("insights");
        assert_eq!(insights.tools[0].p50_ms, Some(30.0), "{insights:?}");
        assert_eq!(insights.tools[0].p95_ms, Some(100.0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn purge_before_deletes_only_older_rows() {
        let (db, path) = temp_db();
        let repo = AuditLogRepository::new(&db);
        repo.insert(
            "old",
            "2026-01-01T00:00:00Z",
            None,
            None,
            "search",
            None,
            None,
            None,
            1,
            None,
        )
        .expect("insert");
        repo.insert(
            "keep",
            "2026-03-01T00:00:00Z",
            None,
            None,
            "search",
            None,
            None,
            None,
            2,
            None,
        )
        .expect("insert");

        let deleted = repo.purge_before("2026-02-01T00:00:00Z").expect("purge");

        assert_eq!(deleted, 1);
        assert_eq!(repo.get_stats().expect("stats").total_calls, 1);

        let _ = std::fs::remove_file(path);
    }
}
