//! Axum middleware — extract CurrentUser from JWT cookie.
//!
//! NOTE: Full JWT validation implementation tracked in issue #2.
//! This module currently provides a placeholder that compiles but rejects all requests.
//! Issue #2 will replace this with proper `FromRequestParts<AppState>` + `validate_jwt`.

#![allow(unused_imports)]

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// Extract current user from session cookie.
/// Returns 401 if not authenticated.
///
/// TODO(issue #2): implement `FromRequestParts<AppState>` that reads the `session` cookie
/// and calls `rungu_auth::session::validate_jwt(token, state.config.auth.app_secret)`.
pub struct CurrentUser;

impl FromRequestParts<()> for CurrentUser {
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(_parts: &mut Parts, _state: &()) -> Result<Self, Self::Rejection> {
        // Placeholder — proper implementation in issue #2.
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
