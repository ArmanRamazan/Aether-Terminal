//! Web error types with automatic HTTP response conversion.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors produced by the web server.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("internal server error")]
    Internal,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let status = match &self {
            WebError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            WebError::NotFound(_) => StatusCode::NOT_FOUND,
            WebError::BadRequest(_) => StatusCode::BAD_REQUEST,
        };
        let body = serde_json::json!({ "error": self.to_string() });
        (status, axum::Json(body)).into_response()
    }
}
