use crate::sidecar::db::audit_log_repo::{normalize_time_window, AuditLogRepository};
use crate::sidecar::http::app_error::AppError;
use crate::sidecar::http::AppState;
use axum::{
    extract::{Query, State},
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/logs", get(list))
        .route("/api/logs/stats", get(stats))
        .route("/api/logs/insights", get(insights))
}

#[derive(Deserialize)]
struct LogQuery {
    server_id: Option<String>,
    tool_name: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LogQuery>,
) -> Result<Json<Value>, AppError> {
    let (from, to) = normalize_time_window(query.from.as_deref(), query.to.as_deref())
        .map_err(AppError::validation)?;
    let repo = AuditLogRepository::new(&state.db);
    let logs = repo
        .query_logs(
            query.server_id.as_deref(),
            query.tool_name.as_deref(),
            from.as_deref(),
            to.as_deref(),
            query.limit,
            query.offset,
        )
        .map_err(AppError::internal)?;
    Ok(Json(
        serde_json::to_value(logs).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
struct InsightsQuery {
    from: Option<String>,
    to: Option<String>,
}

async fn insights(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InsightsQuery>,
) -> Result<Json<Value>, AppError> {
    let (from, to) = normalize_time_window(query.from.as_deref(), query.to.as_deref())
        .map_err(AppError::validation)?;
    let repo = AuditLogRepository::new(&state.db);
    let insights = repo
        .get_insights(from.as_deref(), to.as_deref())
        .map_err(AppError::internal)?;
    Ok(Json(
        serde_json::to_value(insights).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

async fn stats(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let repo = AuditLogRepository::new(&state.db);
    let stats = repo.get_stats().map_err(AppError::internal)?;
    Ok(Json(
        serde_json::to_value(stats).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::audit_log_repo::AuditLogRepository;
    use crate::sidecar::db::server_repo::ServerRepository;
    use crate::sidecar::db::Database;
    use axum::body::{to_bytes, Body};
    use std::sync::Arc;
    use std::time::SystemTime;
    use tower::ServiceExt;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-logs-route-{test_name}-{timestamp}"))
    }

    fn test_state(data_dir: std::path::PathBuf) -> Arc<AppState> {
        AppState::for_test(&data_dir)
    }

    fn insert_server(db: &Database, id: &str, name: &str) {
        use crate::sidecar::db::server_repo::ServerInsertInput;
        ServerRepository::new(db)
            .insert_one_with_id(
                id,
                0,
                &ServerInsertInput {
                    name: name.into(),
                    connection_type: "stdio".into(),
                    command: Some("node".into()),
                    args: Some(vec![]),
                    url: None,
                    env: None,
                    headers: None,
                    working_dir: None,
                    auto_start: false,
                },
            )
            .expect("failed to insert server");
    }

    #[tokio::test]
    async fn stats_handles_empty_audit_log_table() {
        let data_dir = temp_data_dir("empty-stats");
        let state = test_state(data_dir.clone());

        let Json(value) = stats(State(state)).await.expect("stats should succeed");

        assert_eq!(value["totalCalls"], 0);
        assert_eq!(value["errorCalls"], 0);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn list_accepts_snake_case_frontend_filters() {
        let data_dir = temp_data_dir("snake-case-filters");
        let state = test_state(data_dir.clone());
        insert_server(&state.db, "server-a", "Alpha");
        insert_server(&state.db, "server-b", "Beta");
        let repo = AuditLogRepository::new(&state.db);
        repo.insert(
            "log-a",
            "2026-01-01T00:00:00Z",
            None,
            Some("server-a"),
            "search",
            None,
            None,
            None,
            10,
            None,
        )
        .expect("failed to insert first log");
        repo.insert(
            "log-b",
            "2026-01-01T00:00:01Z",
            None,
            Some("server-b"),
            "search",
            None,
            None,
            None,
            10,
            None,
        )
        .expect("failed to insert second log");

        let response = router()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/logs?server_id=server-a")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body should read");
        let logs: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be json");

        assert_eq!(logs.as_array().expect("logs should be array").len(), 1);
        assert_eq!(logs[0]["serverId"], "server-a");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn list_and_insights_validate_and_share_half_open_windows() {
        let data_dir = temp_data_dir("time-windows");
        let state = test_state(data_dir.clone());
        let repo = AuditLogRepository::new(&state.db);
        for (id, timestamp) in [
            ("from", "2026-01-01T00:00:01+00:00"),
            ("to", "2026-01-01T00:00:02Z"),
        ] {
            repo.insert(
                id, timestamp, None, None, "search", None, None, None, 10, None,
            )
            .unwrap();
        }
        let app = router().with_state(state);
        for path in ["/api/logs", "/api/logs/insights"] {
            for query in [
                "from=garbage",
                "to=garbage",
                "from=2026-01-02T00:00:00Z&to=2026-01-01T00:00:00Z",
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        axum::http::Request::builder()
                            .uri(format!("{path}?{query}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
            }
            for (to, expected) in [("2026-01-01T00:00:02Z", 1), ("2026-01-01T00:00:01Z", 0)] {
                let response = app
                    .clone()
                    .oneshot(
                        axum::http::Request::builder()
                            .uri(format!("{path}?from=2026-01-01T00:00:01Z&to={to}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::OK);
                let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                let count = if path == "/api/logs" {
                    value.as_array().unwrap().len() as u64
                } else {
                    value["totalCalls"].as_u64().unwrap()
                };
                assert_eq!(count, expected);
            }
        }
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn insights_returns_grouped_rows_over_http() {
        let data_dir = temp_data_dir("insights-http");
        let state = test_state(data_dir.clone());
        insert_server(&state.db, "server-a", "Alpha");
        insert_server(&state.db, "server-b", "Beta");
        let repo = AuditLogRepository::new(&state.db);
        repo.insert(
            "log-a",
            "2026-01-01T00:00:00Z",
            None,
            Some("server-a"),
            "search",
            None,
            None,
            Some("boom"),
            10,
            None,
        )
        .expect("failed to insert first log");
        repo.insert(
            "log-b",
            "2026-01-02T00:00:00Z",
            None,
            Some("server-a"),
            "search",
            None,
            None,
            None,
            20,
            None,
        )
        .expect("failed to insert second log");

        let response = router()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/logs/insights")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body should read");
        let insights: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be json");

        assert_eq!(insights["totalCalls"], 2);
        assert_eq!(insights["tools"].as_array().expect("tools").len(), 1);
        assert_eq!(insights["tools"][0]["serverName"], "Alpha");
        assert_eq!(insights["tools"][0]["p50Ms"], 10.0);
        assert_eq!(insights["tools"][0]["p95Ms"], 20.0);
        assert_eq!(insights["servers"].as_array().expect("servers").len(), 1);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
