use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    client_message: Option<String>,
}

impl AppError {
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn upstream(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message)
            .with_client_message("Unable to check for updates".to_string())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", msg)
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg)
    }

    pub fn order_invalid(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "ORDER_INVALID", msg)
    }

    pub fn active_profile(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "ACTIVE_PROFILE", msg)
    }

    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "PAYLOAD_TOO_LARGE", msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
    }

    pub fn internal_public(msg: impl Into<String>, client_msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
            .with_client_message(client_msg)
    }

    fn new(status: StatusCode, code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: msg.into(),
            client_message: None,
        }
    }

    fn with_client_message(mut self, msg: impl Into<String>) -> Self {
        self.client_message = Some(msg.into());
        self
    }

    #[cfg(test)]
    pub fn status_code(&self) -> StatusCode {
        self.status
    }

    #[cfg(test)]
    pub fn code(&self) -> &str {
        self.code
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // 5xx 可能包含底层细节；默认隐藏，只返回显式标记为可公开的消息。
        let client_message = if self.status.is_server_error() {
            tracing::error!(code = self.code, error = %self.message, "Request handling failed");
            self.client_message
                .unwrap_or_else(|| "Internal server error".to_string())
        } else {
            self.message
        };
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "message": client_message } })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_uses_bad_request_status_and_code() {
        let err = AppError::validation("advanced.sidecarPort must be between 1024 and 65535");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.code(), "VALIDATION_ERROR");
        assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn internal_uses_server_error_status_and_code() {
        let err = AppError::internal("boom");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code(), "INTERNAL_ERROR");
    }
}
