use crate::sidecar::http::{app_error::AppError, AppState};
use crate::sidecar::services::server_authorization::{AuthorizationSnapshot, RevocationResult};
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/servers/{id}/authorization",
            get(snapshot).delete(revoke),
        )
        .route(
            "/api/servers/{id}/authorization/client",
            put(register_client),
        )
        .route("/api/servers/{id}/authorization/start", post(start))
}

async fn snapshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AuthorizationSnapshot>, AppError> {
    state
        .server_manager
        .authorization()
        .snapshot(&id)
        .await
        .map(Json)
        .map_err(AppError::internal)
}

async fn revoke(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RevocationResult>, AppError> {
    state
        .server_manager
        .authorization()
        .revoke(&id)
        .await
        .map(Json)
        .map_err(AppError::credential_cleanup)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientBody {
    client_id: String,
    client_secret: Option<String>,
}

async fn register_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ClientBody>,
) -> Result<Json<AuthorizationSnapshot>, AppError> {
    state
        .server_manager
        .authorization()
        .register_client(&id, &body.client_id, body.client_secret.as_deref())
        .await
        .map(Json)
        .map_err(AppError::validation)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartBody {
    client_id: Option<String>,
}

async fn start(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<StartBody>>,
) -> Result<Json<Value>, AppError> {
    let client_id = body.and_then(|body| body.0.client_id);
    state
        .server_manager
        .authorization()
        .start(&id, client_id.as_deref())
        .await
        .map(Json)
        .map_err(|error| {
            AppError::upstream(
                "OAUTH_START_FAILED",
                error,
                "Failed to start OAuth authorization. Check the Server and try again.",
            )
        })
}
