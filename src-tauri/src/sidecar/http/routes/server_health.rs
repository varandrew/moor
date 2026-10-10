use crate::sidecar::{
    http::{app_error::AppError, AppState},
    services::server_manager::{HealthCheckError, ServerHealthSnapshot},
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/servers/health", get(snapshots))
        .route("/api/servers/{id}/check-health", post(check))
}

async fn snapshots(State(state): State<Arc<AppState>>) -> Json<Vec<ServerHealthSnapshot>> {
    Json(state.server_manager.health_snapshots().await)
}

async fn check(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ServerHealthSnapshot>, AppError> {
    state
        .server_manager
        .check_health(&id, true)
        .await
        .map(Json)
        .map_err(|error| match error {
            HealthCheckError::NotFound => AppError::not_found("Server not found"),
            HealthCheckError::NotRunning => {
                AppError::conflict("SERVER_NOT_RUNNING", "Server is not running")
            }
            HealthCheckError::Busy => AppError::conflict(
                "SERVER_BUSY",
                "Server is busy; try again when the tool call finishes",
            ),
            HealthCheckError::Changed => AppError::conflict(
                "SERVER_CHANGED",
                "Server session changed during the health check",
            ),
        })
}
