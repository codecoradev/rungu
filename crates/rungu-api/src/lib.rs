//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.
//!
//! ## Module structure
//!
//! Each resource has its own route module (`xxx_routes.rs`) that exposes a
//! `router()` function returning `Router<AppState>`. The [`api_routes`]
//! function merges them all together.
//!
//! ## Adding new routes (for issues #5, #6, #7)
//!
//! 1. Create `crates/rungu-api/src/vote_routes.rs` (or similar)
//! 2. Expose `pub fn router() -> Router<AppState>`
//! 3. Register here: `pub mod vote_routes;` + `.merge(vote_routes::router())`
//! 4. Use `crate::error::ApiError` for consistent error handling

pub mod auth_routes;
pub mod comment_routes;
pub mod error;
pub mod oauth;
pub mod post_routes;

use axum::Router;
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

/// Build the complete API router.
///
/// This is the single entry point — `rungud::server` calls this and nests
/// it under `/api`.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Auth (login, callback, logout, me, providers)
        .merge(auth_routes::auth_routes())
        // Posts CRUD
        .merge(post_routes::router())
        // Comments
        .merge(comment_routes::router())
    // Future routes — just add .merge() here:
    // .merge(vote_routes::router())    // issue #5
    // .merge(project_routes::router()) // issue #7
}
