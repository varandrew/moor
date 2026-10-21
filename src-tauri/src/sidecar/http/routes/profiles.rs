use crate::sidecar::http::app_error::AppError;
use crate::sidecar::http::AppState;
use crate::sidecar::services::profile_service::ProfileService;
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/profiles", get(list).post(create))
        .route(
            "/api/profiles/{id}",
            get(get_one).put(update).delete(remove),
        )
        .route("/api/profiles/{id}/activate", put(activate))
        .route(
            "/api/profiles/{profileId}/servers/{serverId}",
            get(get_profile_server).put(upsert_profile_server),
        )
        .route("/api/profiles/{id}/clone", post(clone))
        .route("/api/profiles/{id}/tools", get(profile_tools))
        .route("/api/profiles/{id}/servers-state", put(bulk_upsert_servers))
}

async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let profiles = ProfileService::list(&state.db).map_err(AppError::from)?;
    Ok(Json(
        serde_json::to_value(profiles).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
}

async fn create(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<CreateBody>,
) -> Result<(axum::http::StatusCode, Json<Value>), AppError> {
    if body.name.is_empty() {
        return Err(AppError::validation("name is required"));
    }
    let profile = ProfileService::create(&state.db, &body.name).map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::to_value(profile).map_err(|e| AppError::internal(e.to_string()))?),
    ))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let (profile, servers) = ProfileService::get_detail(&state.db, &id).map_err(AppError::from)?;
    let mut profile_value =
        serde_json::to_value(profile).map_err(|e| AppError::internal(e.to_string()))?;
    if let Some(obj) = profile_value.as_object_mut() {
        obj.insert(
            "servers".to_string(),
            serde_json::to_value(servers).map_err(|e| AppError::internal(e.to_string()))?,
        );
    }
    Ok(Json(profile_value))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBody {
    name: Option<String>,
    exposure_mode: Option<String>,
    risk_policy: Option<String>,
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<UpdateBody>,
) -> Result<Json<Value>, AppError> {
    let touches_exposure = body.exposure_mode.is_some() || body.risk_policy.is_some();
    let profile = ProfileService::update(
        &state.db,
        &id,
        body.name.as_deref(),
        body.exposure_mode.as_deref(),
        body.risk_policy.as_deref(),
    )
    .map_err(AppError::from)?;
    if touches_exposure {
        state
            .event_bus
            .emit(crate::sidecar::services::event_bus::Evt::ToolsChanged {
                reason: "profile".into(),
            });
    }
    Ok(Json(
        serde_json::to_value(profile).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

async fn remove(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    ProfileService::remove(&state.db, &id).map_err(AppError::from)?;
    Ok(Json(json!({ "success": true })))
}

async fn activate(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let profile =
        ProfileService::activate(&state.db, &state.event_bus, &id).map_err(AppError::from)?;
    Ok(Json(
        serde_json::to_value(profile).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

async fn get_profile_server(
    State(state): State<Arc<AppState>>,
    Path((profile_id, server_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let server = ProfileService::get_profile_server(&state.db, &profile_id, &server_id)
        .map_err(AppError::from)?;
    Ok(Json(
        serde_json::to_value(server).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertProfileServerBody {
    enabled: Option<bool>,
    disabled_tools: Option<Vec<String>>,
}

async fn upsert_profile_server(
    State(state): State<Arc<AppState>>,
    Path((profile_id, server_id)): Path<(String, String)>,
    axum::Json(body): axum::Json<UpsertProfileServerBody>,
) -> Result<Json<Value>, AppError> {
    let result = ProfileService::upsert_profile_server(
        &state.db,
        &profile_id,
        &server_id,
        body.enabled,
        body.disabled_tools.as_ref(),
    )
    .map_err(AppError::from)?;
    Ok(Json(
        serde_json::to_value(result).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
struct CloneBody {
    name: String,
}

async fn clone(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<CloneBody>,
) -> Result<(axum::http::StatusCode, Json<Value>), AppError> {
    let profile =
        ProfileService::clone_profile(&state.db, &id, &body.name).map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::to_value(profile).map_err(|e| AppError::internal(e.to_string()))?),
    ))
}

async fn profile_tools(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let groups = ProfileService::get_profile_tool_groups(&state.db, &id).map_err(AppError::from)?;
    Ok(Json(
        serde_json::to_value(groups).map_err(|e| AppError::internal(e.to_string()))?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkServerStateBody {
    updates: Vec<BulkServerStateUpdate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkServerStateUpdate {
    server_id: String,
    enabled: Option<bool>,
    disabled_tools: Option<Vec<String>>,
    pinned_tools: Option<Vec<String>>,
}

async fn bulk_upsert_servers(
    State(state): State<Arc<AppState>>,
    Path(profile_id): Path<String>,
    axum::Json(body): axum::Json<BulkServerStateBody>,
) -> Result<Json<Value>, AppError> {
    let items: Vec<crate::sidecar::db::profile_repo::ProfileServerUpsertInput> = body
        .updates
        .into_iter()
        .map(
            |update| crate::sidecar::db::profile_repo::ProfileServerUpsertInput {
                server_id: update.server_id,
                enabled: update.enabled,
                disabled_tools: update.disabled_tools,
                pinned_tools: update.pinned_tools,
            },
        )
        .collect();
    ProfileService::upsert_profile_servers_batch(&state.db, &profile_id, &items)
        .map_err(AppError::from)?;
    state
        .event_bus
        .emit(crate::sidecar::services::event_bus::Evt::ToolsChanged {
            reason: "governance".into(),
        });
    Ok(Json(json!({ "success": true, "updated": items.len() })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::profile_repo::ProfileRepository;
    use axum::body::Body;
    use std::sync::Arc;
    use std::time::SystemTime;
    use tower::ServiceExt;

    fn temp_data_dir(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-profiles-route-{test_name}-{timestamp}"))
    }

    fn test_state(data_dir: std::path::PathBuf) -> Arc<AppState> {
        AppState::for_test(&data_dir)
    }

    #[tokio::test]
    async fn activate_profile_accepts_frontend_put_route() {
        let data_dir = temp_data_dir("activate-put");
        let state = test_state(data_dir.clone());
        let repo = ProfileRepository::new(&state.db);
        repo.seed_default().expect("failed to seed profile");
        let profile = repo.create("Work").expect("failed to create profile");

        let response = router()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::PUT)
                    .uri(format!("/api/profiles/{}/activate", profile.id))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn clone_copies_server_states_as_inactive_profile() {
        let data_dir = temp_data_dir("clone-profile");
        let state = test_state(data_dir.clone());
        let repo = ProfileRepository::new(&state.db);
        repo.seed_default().expect("failed to seed profile");
        use crate::sidecar::db::server_repo::{ServerInsertInput, ServerRepository};
        ServerRepository::new(&state.db)
            .insert_one_with_id(
                "srv-a",
                0,
                &ServerInsertInput {
                    name: "Alpha".into(),
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
            .expect("failed to insert server");
        let active = repo.find_active_id().expect("active id").expect("active");
        repo.upsert_profile_server(&active, "srv-a", Some(false), Some(&vec!["t1".into()]))
            .expect("failed to set state");

        let response = router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri(format!("/api/profiles/{active}/clone"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "name": "Copy" }).to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let cloned: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let cloned_id = cloned["id"].as_str().expect("id").to_string();

        let (profile, servers) = ProfileService::get_detail(&state.db, &cloned_id).expect("detail");
        assert_eq!(profile.name, "Copy");
        assert!(!profile.is_active);
        assert_eq!(servers.len(), 1);
        assert!(!servers[0].profile_server.enabled);
        assert_eq!(servers[0].profile_server.disabled_tools, vec!["t1"]);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn bulk_servers_state_applies_partial_updates_atomically() {
        let data_dir = temp_data_dir("bulk-servers-state");
        let state = test_state(data_dir.clone());
        let repo = ProfileRepository::new(&state.db);
        repo.seed_default().expect("failed to seed profile");
        use crate::sidecar::db::server_repo::{ServerInsertInput, ServerRepository};
        for id in ["srv-a", "srv-b"] {
            ServerRepository::new(&state.db)
                .insert_one_with_id(
                    id,
                    0,
                    &ServerInsertInput {
                        name: format!("Server {id}"),
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
                .expect("failed to insert server");
        }
        let active = repo.find_active_id().expect("active id").expect("active");
        repo.upsert_profile_server(&active, "srv-a", Some(true), None)
            .expect("seed srv-a");

        let payload = serde_json::json!({ "updates": [
            { "serverId": "srv-a", "disabledTools": ["t1", "t2"] },
            { "serverId": "srv-b", "enabled": false }
        ] });
        let response = router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::PUT)
                    .uri(format!("/api/profiles/{active}/servers-state"))
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let (_, servers) = ProfileService::get_detail(&state.db, &active).expect("detail");
        let a = servers
            .iter()
            .find(|s| s.server.id == "srv-a")
            .expect("srv-a");
        let b = servers
            .iter()
            .find(|s| s.server.id == "srv-b")
            .expect("srv-b");
        // 补丁语义：srv-a 只改 disabledTools，enabled 保留 true；srv-b 只改 enabled
        assert!(a.profile_server.enabled);
        assert_eq!(a.profile_server.disabled_tools, vec!["t1", "t2"]);
        assert!(!b.profile_server.enabled);
        assert!(b.profile_server.disabled_tools.is_empty());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn bulk_servers_state_rejects_unknown_server_and_writes_nothing() {
        let data_dir = temp_data_dir("bulk-servers-state-fk");
        let state = test_state(data_dir.clone());
        let repo = ProfileRepository::new(&state.db);
        repo.seed_default().expect("failed to seed profile");
        use crate::sidecar::db::server_repo::{ServerInsertInput, ServerRepository};
        ServerRepository::new(&state.db)
            .insert_one_with_id(
                "srv-a",
                0,
                &ServerInsertInput {
                    name: "Server srv-a".into(),
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
            .expect("failed to insert server");
        let active = repo.find_active_id().expect("active id").expect("active");
        repo.upsert_profile_server(&active, "srv-a", Some(true), None)
            .expect("seed srv-a");

        // 混入未知 serverId：整批 400 拒绝，合法项不得落任何写入
        let payload = serde_json::json!({ "updates": [
            { "serverId": "srv-a", "disabledTools": ["t1"] },
            { "serverId": "srv-ghost", "enabled": false }
        ] });
        let response = router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::PUT)
                    .uri(format!("/api/profiles/{active}/servers-state"))
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let (_, servers) = ProfileService::get_detail(&state.db, &active).expect("detail");
        let a = servers
            .iter()
            .find(|s| s.server.id == "srv-a")
            .expect("srv-a");
        assert!(a.profile_server.enabled);
        assert_eq!(a.profile_server.disabled_tools, Vec::<String>::new());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn profile_tools_returns_grouped_matrix_with_disabled_flags() {
        let data_dir = temp_data_dir("profile-tools");
        let state = test_state(data_dir.clone());
        let repo = ProfileRepository::new(&state.db);
        repo.seed_default().expect("failed to seed profile");
        use crate::sidecar::db::server_repo::{ServerInsertInput, ServerRepository};
        use crate::sidecar::db::tool_discovery_repo::{ToolDiscoveryRepository, ToolInsert};
        ServerRepository::new(&state.db)
            .insert_one_with_id(
                "srv-a",
                0,
                &ServerInsertInput {
                    name: "Alpha".into(),
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
            .expect("failed to insert server");
        ToolDiscoveryRepository::new(&state.db)
            .replace_tools_for_server(
                "srv-a",
                &[
                    ToolInsert {
                        name: "search".into(),
                        description: None,
                        input_schema: None,
                    },
                    ToolInsert {
                        name: "fetch".into(),
                        description: None,
                        input_schema: None,
                    },
                ],
            )
            .expect("failed to insert tools");
        let active = repo.find_active_id().expect("active id").expect("active");
        repo.upsert_profile_server(&active, "srv-a", Some(true), Some(&vec!["fetch".into()]))
            .expect("failed to disable fetch");

        let response = router()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/profiles/{active}/tools"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("route should respond");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let groups: serde_json::Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(groups.as_array().expect("groups").len(), 1);
        assert_eq!(groups[0]["serverName"], "Alpha");
        assert_eq!(groups[0]["serverEnabled"], true);
        let tools = groups[0]["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 2);
        let fetch = tools
            .iter()
            .find(|t| t["toolName"] == "fetch")
            .expect("fetch");
        assert_eq!(fetch["disabled"], true);
        let search = tools
            .iter()
            .find(|t| t["toolName"] == "search")
            .expect("search");
        assert_eq!(search["disabled"], false);

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
