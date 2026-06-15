//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.

pub mod auth_routes;

use rungu_core::Store;

/// Shared application state for API handlers.
/// Clone-able: fields are Arc-wrapped internally by `Store` and `Config`.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: rungu_auth::AuthConfig,
}
