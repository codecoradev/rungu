//! Shared API error types.
//!
//! All route handlers return `Result<T, ApiError>`. This ensures consistent
//! error responses across all endpoint modules.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::error::ApiError;
//!
//! async fn handler() -> Result<impl IntoResponse, ApiError> {
//!     let item = find_item().ok_or_else(|| ApiError::not_found("Item not found"))?;
//!     Ok(Json(json!({ "data": item })))
//! }
//! ```

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Standard API error.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: msg.into() }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: msg.into() }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::FORBIDDEN, message: msg.into() }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: msg.into() }
    }
}

/// Convert `anyhow::Error` to `ApiError` (500).
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = %e, "API error");
        Self::internal("Internal server error")
    }
}

/// Convert `serde_json::Error` to `ApiError` (400).
impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::bad_request(format!("JSON error: {e}"))
    }
}

/// Render error as `{ "error": "message" }` with appropriate status code.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}
