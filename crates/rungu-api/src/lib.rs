//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.

pub mod auth_routes;
pub mod oauth;

use axum::extract::FromRef;
use rungu_core::Store;

/// Shared application state for API handlers.
/// Clone-able: fields are Arc-wrapped internally by `Store` and `Config`.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: rungu_auth::AuthConfig,
}

/// Implement `FromRef` so that `rungu_auth` extractors (`CurrentUser`, `OptionalCurrentUser`)
/// can obtain `AuthConfig` from `AppState` without a circular dependency.
impl FromRef<AppState> for rungu_auth::AuthConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}
