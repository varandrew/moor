use crate::sidecar::{
    http::{app_error::AppError, AppState},
    services::updates::{check_update, UpdateCheck},
};
use axum::{extract::State, routing::get, Json, Router};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/updates/check", get(check))
}

async fn check(State(state): State<Arc<AppState>>) -> Result<Json<UpdateCheck>, AppError> {
    check_update(&state.version)
        .await
        .map(Json)
        .map_err(|error| AppError::upstream("UPDATE_CHECK_FAILED", error))
}
