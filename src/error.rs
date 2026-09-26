use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CloakdError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[allow(dead_code)]
    #[error("Invalid client request: {0}")]
    InvalidRequest(String),

    #[error("Upstream error: {status} - {message}")]
    Upstream {
        status: StatusCode,
        message: String,
    },

    #[error("Stream processing error: {0}")]
    Stream(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for CloakdError {
    fn into_response(self) -> Response {
        let (status, err_type, message) = match self {
            CloakdError::InvalidRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                msg,
            ),
            CloakdError::Upstream { status, message } => (
                status,
                "upstream_error",
                message,
            ),
            CloakdError::Config(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_configuration_error",
                msg,
            ),
            CloakdError::Stream(msg) => (
                StatusCode::BAD_GATEWAY,
                "stream_processing_error",
                msg,
            ),
            CloakdError::Serialization(err) => (
                StatusCode::BAD_REQUEST,
                "json_serialization_error",
                err.to_string(),
            ),
            CloakdError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                err.to_string(),
            ),
        };

        // Strict OpenAI JSON error format compatibility
        let body = json!({
            "error": {
                "message": message,
                "type": err_type,
                "code": status.as_u16(),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}
