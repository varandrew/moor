use crate::sidecar::db::server_repo::Server;
use crate::sidecar::http::app_error::AppError;
use crate::sidecar::http::AppState;
use crate::sidecar::services::server_manager::public_server_start_error_message;
use crate::sidecar::services::server_service::{
    CreateServerInput, ServerService, UpdateServerInput,
};
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, patch, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/servers", get(list).post(create))
        .route("/api/servers/order", put(reorder))
        .route("/api/servers/{id}", get(get_one).put(update).delete(remove))
        .route("/api/servers/{id}/start", post(start))
        .route("/api/servers/{id}/stop", post(stop))
        .route("/api/servers/{id}/tools", get(tools))
        .route("/api/servers/{id}/tools/refresh", post(refresh_tools))
        .route(
            "/api/servers/{id}/tools/{toolName}/risk",
            patch(set_tool_risk),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RiskBody {
    risk_level: String,
}

/// 对运行中的 Server 重新发现工具，不重启进程。
async fn refresh_tools(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let count = state
        .server_manager
        .refresh_server_tools(&id)
        .await
        .map_err(|error| {
            use crate::sidecar::services::server_manager::ToolRefreshError;
            match error {
                ToolRefreshError::NotFound => AppError::not_found("Server not found"),
                ToolRefreshError::NotRunning => {
                    AppError::conflict("SERVER_NOT_RUNNING", "Server is not running")
                }
                ToolRefreshError::Changed => AppError::conflict(
                    "SERVER_CHANGED",
                    "Server changed while refreshing tools. Refresh again.",
                ),
                ToolRefreshError::Upstream(message) => AppError::upstream(
                    "TOOL_DISCOVERY_FAILED",
                    message,
                    "Failed to refresh tools from the server.",
                ),
                ToolRefreshError::Storage(message) => {
                    AppError::internal_public(message, "Failed to save discovered tools.")
                }
            }
        })?;
    Ok(Json(serde_json::json!({ "tools": count })))
}

/// 用户手动纠正风险等级。启发式一定会误判，这个入口是纠错通道。
async fn set_tool_risk(
    State(state): State<Arc<AppState>>,
    Path((server_id, tool_name)): Path<(String, String)>,
    axum::Json(body): axum::Json<RiskBody>,
) -> Result<Json<Value>, AppError> {
    let level = crate::sidecar::services::tool_risk::RiskLevel::from_db(&body.risk_level);
    if level == crate::sidecar::services::tool_risk::RiskLevel::Unknown
        && body.risk_level != "unknown"
    {
        return Err(AppError::validation(
            "riskLevel must be safe, write, destructive or unknown",
        ));
    }
    crate::sidecar::db::tool_discovery_repo::ToolDiscoveryRepository::new(&state.db)
        .set_tool_risk(&server_id, &tool_name, level)
        .map_err(AppError::not_found)?;
    state
        .event_bus
        .emit(crate::sidecar::services::event_bus::Evt::ToolsChanged {
            reason: "risk".into(),
        });
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Server>>, AppError> {
    let servers = ServerService::list_servers(&state.db).map_err(AppError::internal)?;
    Ok(Json(servers))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateServerBody {
    name: String,
    connection_type: String,
    command: Option<String>,
    args: Option<Vec<String>>,
    url: Option<String>,
    env: Option<std::collections::HashMap<String, String>>,
    headers: Option<std::collections::HashMap<String, String>>,
    working_dir: Option<String>,
    auto_start: Option<bool>,
}

async fn create(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<CreateServerBody>,
) -> Result<(axum::http::StatusCode, Json<Server>), AppError> {
    let input = CreateServerInput {
        name: body.name,
        connection_type: body.connection_type,
        command: body.command,
        args: body.args,
        url: body.url,
        env: body.env,
        headers: body.headers,
        working_dir: body.working_dir,
        auto_start: body.auto_start.unwrap_or(false),
    };
    input.validate().map_err(AppError::validation)?;

    let server = ServerService::insert_server(&state.db, &state.server_manager, &input)
        .await
        .map_err(AppError::internal)?;

    if server.auto_start {
        let sm = state.server_manager.clone();
        let server_id = server.id.clone();
        tokio::spawn(async move {
            if let Err(err) = sm.start_server(&server_id).await {
                tracing::warn!(
                    server_id = %server_id,
                    error = %err,
                    "auto-start server failed after creation"
                );
            }
        });
    }

    Ok((axum::http::StatusCode::CREATED, Json(server)))
}

#[derive(Deserialize)]
struct ReorderBody {
    #[serde(rename = "serverIds")]
    server_ids: Vec<String>,
}

async fn reorder(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ReorderBody>,
) -> Result<Json<Vec<Server>>, AppError> {
    if body.server_ids.is_empty() {
        return Err(AppError::order_invalid(
            "Server order must include every existing server exactly once.",
        ));
    }
    let servers = ServerService::reorder(&state.db, &body.server_ids).map_err(AppError::from)?;
    Ok(Json(servers))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Server>, AppError> {
    let mut server = ServerService::get_server(&state.db, &id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::not_found("Server not found"))?;

    if let Some(managed) = state.server_manager.get_server(&id).await {
        server.status = managed.status;
    }

    Ok(Json(server))
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<UpdateServerInput>,
) -> Result<Json<Server>, AppError> {
    let server = ServerService::update_server(&state.db, &state.server_manager, &id, &body)
        .await
        .map_err(AppError::from)?;
    Ok(Json(server))
}

async fn remove(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    ServerService::delete_server(&state.db, &state.server_manager, &id)
        .await
        .map_err(AppError::from)?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn start(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    state
        .server_manager
        .start_server(&id)
        .await
        .map_err(|err| {
            AppError::internal_public(err.clone(), public_server_start_error_message(&err))
        })?;
    Ok(Json(serde_json::json!({ "status": "started" })))
}

async fn stop(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    state
        .server_manager
        .stop_server(&id)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(serde_json::json!({ "status": "stopped" })))
}

#[derive(Deserialize)]
struct ToolsQuery {
    profile_id: Option<String>,
}

async fn tools(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ToolsQuery>,
) -> Result<Json<Value>, AppError> {
    let tools = state
        .server_manager
        .get_tool_details(&id, query.profile_id.as_deref())
        .await;
    Ok(Json(
        serde_json::to_value(tools).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::profile_repo::ProfileRepository;
    use crate::sidecar::db::server_repo::ServerRepository;
    use crate::sidecar::db::tool_discovery_repo::{ToolDiscoveryRepository, ToolInsert};
    use crate::sidecar::db::Database;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-server-route-{test_name}-{timestamp}"))
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

    fn fail_profile_server_inserts(db: &Database) {
        db.exec(
            "CREATE TRIGGER fail_profile_server_insert
             BEFORE INSERT ON profile_servers
             BEGIN
               SELECT RAISE(ABORT, 'profile insert failed');
             END;",
        )
        .expect("failed to create failing profile trigger");
    }

    #[tokio::test]
    async fn create_rolls_back_server_when_profile_assignment_fails() {
        let data_dir = temp_data_dir("create-profile-failure");
        let state = test_state(data_dir.clone());
        ProfileRepository::new(&state.db)
            .seed_default()
            .expect("failed to seed profile");
        fail_profile_server_inserts(&state.db);

        let result = create(
            State(state.clone()),
            axum::Json(CreateServerBody {
                name: "Broken".to_string(),
                connection_type: "stdio".to_string(),
                command: Some("node".to_string()),
                args: None,
                url: None,
                env: None,
                headers: None,
                working_dir: None,
                auto_start: None,
            }),
        )
        .await;

        assert!(result.is_err());
        let ids = state
            .db
            .query_all("SELECT id FROM mcp_servers", &[], |row| {
                row.get::<_, String>(0)
            })
            .expect("failed to query servers");
        for id in &ids {
            assert!(state.server_manager.get_server(id).await.is_none());
        }
        assert!(ids.is_empty());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn tools_route_returns_disabled_tools_with_callable_exposed_names() {
        let data_dir = temp_data_dir("tools-detail");
        let state = test_state(data_dir.clone());
        let profile_repo = ProfileRepository::new(&state.db);
        profile_repo.seed_default().expect("failed to seed profile");
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to find active profile")
            .expect("active profile should exist");

        insert_server(&state.db, "server-a", "Alpha");
        insert_server(&state.db, "server-b", "Beta");
        profile_repo
            .assign_to_active_profile(&["server-a".to_string(), "server-b".to_string()])
            .expect("failed to assign profile servers");
        profile_repo
            .upsert_profile_server(
                &profile_id,
                "server-a",
                Some(true),
                Some(&vec!["search".to_string()]),
            )
            .expect("failed to disable tool");

        let discovered_tools = [ToolInsert {
            name: "search".to_string(),
            description: Some("Search".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
        }];
        let tool_repo = ToolDiscoveryRepository::new(&state.db);
        tool_repo
            .replace_tools_for_server("server-a", &discovered_tools)
            .expect("failed to insert tools for server-a");
        tool_repo
            .replace_tools_for_server("server-b", &discovered_tools)
            .expect("failed to insert tools for server-b");

        let Json(value) = tools(
            State(state),
            Path("server-a".to_string()),
            Query(ToolsQuery {
                profile_id: Some(profile_id),
            }),
        )
        .await
        .expect("tools route should succeed");

        let list = value.as_array().expect("tools response should be array");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["toolName"], "search");
        assert_eq!(list[0]["disabled"], true);
        assert_eq!(list[0]["exposedName"], "alpha__search");

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
