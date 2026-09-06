//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.

pub mod admin_routes;
pub mod analytics;
pub mod attachment_routes;
pub mod auth_routes;
pub mod comment_routes;
pub mod error;
pub mod meta;
pub mod oauth;
pub mod openapi;
pub mod post_routes;
pub mod project_routes;
pub mod vote_routes;
pub mod webhook;
pub mod webhook_routes;

use axum::Router;
use axum::extract::FromRef;
use rungu_core::Store;

/// Shared application state for API handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: rungu_auth::AuthConfig,
    /// Reused HTTP client for outbound calls (OAuth token exchange, userinfo fetch).
    ///
    /// `reqwest::Client` holds a connection pool, DNS cache, and TLS state that is
    /// expensive to rebuild per request. Constructed once at startup and shared
    /// across all handlers that need outbound HTTP.
    pub http_client: reqwest::Client,
    /// Storage backend for file attachments.
    pub storage: std::sync::Arc<dyn rungu_core::Storage>,
    /// Instance branding (white-label, #185) — resolved from env at startup.
    pub branding: crate::meta::InstanceBranding,
    /// License status for the white-label badge (soft gate via Polar).
    pub license: std::sync::Arc<crate::meta::LicenseStatus>,
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
        .merge(attachment_routes::router())
        .merge(webhook_routes::router())
        .merge(admin_routes::router())
        .merge(crate::meta::meta_routes())
}
