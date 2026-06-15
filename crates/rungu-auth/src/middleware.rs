//! Axum middleware — extract CurrentUser from JWT cookie.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use rungu_proto::{CurrentUser, UserRole};

/// Extract current user from session cookie.
/// Returns 401 if not authenticated.
pub struct CurrentUser {
    pub id: String,
    pub email: String,
    pub role: UserRole,
}

impl FromRequestParts<()> for CurrentUser {
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &()) -> Result<Self, Self::Rejection> {
        let cookie = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').find(|s| s.trim().starts_with("session=")))
            .and_then(|s| s.trim().strip_prefix("session="))
            .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

        // TODO: validate JWT using rungu_auth::session::validate_jwt
        // For now, return a placeholder
        let _ = cookie;
        Err(axum::http::StatusCode::UNAUTHORIZED)
    }
}

/// Optional auth — returns None if not authenticated (doesn't reject).
pub struct OptionalCurrentUser {
    pub user: Option<CurrentUser>,
}

impl FromRequestParts<()> for OptionalCurrentUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(_parts: &mut Parts, _state: &()) -> Result<Self, Self::Rejection> {
        Ok(Self { user: None })
    }
}
