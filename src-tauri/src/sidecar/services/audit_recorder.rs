//! 工具调用审计记录器。
//!
//! 封装审计启用检查、活动 Profile 解析、脱敏和写库。审计失败不影响工具调用主路径。

use crate::sidecar::db::audit_log_repo::AuditLogRepository;
use crate::sidecar::db::profile_repo::ProfileRepository;
use crate::sidecar::db::Database;
use crate::sidecar::services::audit_redaction::redact_for_audit;
use crate::sidecar::services::settings::audit_logging_enabled;

/// 一次工具调用的完整记录。调用方填好这些字段,其余(profile 解析、
/// 脱敏、enabled 检查、写库)由 [AuditRecorder::record] 处理。
pub struct ToolCallRecord<'a> {
    pub server_id: Option<&'a str>,
    pub tool_name: &'a str,
    pub arguments: &'a serde_json::Value,
    pub result: Option<&'a serde_json::Value>,
    pub error: Option<&'a str>,
    pub duration_ms: i64,
    pub agent_info: Option<&'a str>,
}

/// 审计记录器。无状态——每次调用从 Database 读取 enabled 标志和活动 profile。
pub struct AuditRecorder;

impl AuditRecorder {
    /// 若审计已开启,则把这条记录(脱敏后)写入 audit_logs;否则直接返回。
    /// 失败静默吞掉(审计是副作用,不应影响工具调用的主路径)。
    pub fn record(db: &Database, entry: ToolCallRecord<'_>) {
        if !audit_logging_enabled(db) {
            return;
        }

        let profile_id = ProfileRepository::new(db).find_active_id().ok().flatten();
        let redacted_args = redact_for_audit(entry.arguments);
        let redacted_result = entry.result.map(redact_for_audit);

        let _ = AuditLogRepository::new(db).insert(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            profile_id.as_deref(),
            entry.server_id,
            entry.tool_name,
            Some(&redacted_args),
            redacted_result.as_ref(),
            entry.error,
            entry.duration_ms,
            entry.agent_info,
        );
    }
}

/// 启动审计日志滚动清理任务：启动立即执行一次，此后每 6 小时一轮。
/// 每轮重新读取设置，logRetentionDays 的修改无需重启（下一轮生效）；0 = 永久保留。
/// 清理失败只告警不重试——残留数据比中断网关更安全。
pub fn spawn_retention_sweeper(db: std::sync::Arc<Database>) {
    use tokio::time::{interval, MissedTickBehavior};

    tokio::spawn(async move {
        let mut ticker = interval(std::time::Duration::from_secs(6 * 60 * 60));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let db = db.clone();
            let deleted = tokio::task::spawn_blocking(move || purge_expired(&db))
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "Audit retention sweep task failed");
                    0
                });
            if deleted > 0 {
                tracing::info!(deleted, "Purged expired audit log entries");
            }
        }
    });
}

fn purge_expired(db: &Database) -> usize {
    let days = crate::sidecar::services::settings::audit_retention_days(db);
    if days == 0 {
        return 0;
    }
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
    match AuditLogRepository::new(db).purge_before(&cutoff) {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!(error = %e, "Audit retention purge failed");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_zero_preserves_history_and_setting_changes_take_effect() {
        let data_dir =
            std::env::temp_dir().join(format!("moor-retention-{}", uuid::Uuid::new_v4()));
        let state = crate::sidecar::http::AppState::for_test(&data_dir);
        let repo = AuditLogRepository::new(&state.db);
        for (id, timestamp) in [
            ("old", "2000-01-01T00:00:00Z"),
            ("recent", &chrono::Utc::now().to_rfc3339()),
        ] {
            repo.insert(
                id, timestamp, None, None, "search", None, None, None, 1, None,
            )
            .unwrap();
        }
        crate::sidecar::services::settings::update_settings(
            &state.db,
            serde_json::json!({"advanced": {"logRetentionDays": 0}}),
        )
        .unwrap();
        assert_eq!(purge_expired(&state.db), 0);
        assert_eq!(repo.get_stats().unwrap().total_calls, 2);
        crate::sidecar::services::settings::update_settings(
            &state.db,
            serde_json::json!({"advanced": {"logRetentionDays": 30}}),
        )
        .unwrap();
        assert_eq!(purge_expired(&state.db), 1);
        assert_eq!(
            repo.query_logs(None, None, None, None, None, None).unwrap()[0].id,
            "recent"
        );
        assert_eq!(purge_expired(&state.db), 0);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
