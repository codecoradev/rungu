//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.

pub mod auth_routes;
pub mod comment_routes;
pub mod error;
pub mod oauth;
pub mod openapi;
pub mod post_routes;
pub mod project_routes;
pub mod vote_routes;

use axum::Router;
use axum::extract::FromRef;
use rungu_core::Store;

/// Shared application state for API handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: rungu_auth::AuthConfig,
}

impl FromRef<AppState> for rungu_auth::AuthConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

/// Auth routes — mounted at root level (NOT under /api).
/// OAuth callback URLs need to be at `/auth/:provider/callback` for redirect URIs.
pub fn auth_routes() -> Router<AppState> {
    auth_routes::auth_routes()
}

/// API routes — mounted under `/api`.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(project_routes::router())
        .merge(post_routes::router())
        .merge(vote_routes::router())
        .merge(comment_routes::router())
}
