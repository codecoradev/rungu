//! # rungu-api
//!
//! REST API routes — Axum handlers for projects, posts, votes, comments, auth.

pub mod admin_routes;
pub mod analytics;
pub mod attachment_routes;
pub mod auth_routes;
pub mod background;
pub mod comment_routes;
pub mod error;
pub mod mcp_http;
pub mod meta;
pub mod oauth;
pub mod openapi;
pub mod post_routes;
pub mod project_routes;
pub mod vote_routes;
pub mod webhook;
pub mod webhook_routes;

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use rungu_core::{EventBus, Operations, Storage, Store};

/// The production event subscribers: webhooks and analytics counters.
/// Shared by the HTTP server and the stdio MCP process.
pub fn default_event_bus(store: &Store, http_client: &reqwest::Client) -> EventBus {
    EventBus::new(vec![
        Arc::new(webhook::WebhookSink::new(store.clone(), http_client.clone())),
        Arc::new(analytics::AnalyticsSink::new(store.clone())),
    ])
}

/// Shared application state for API handlers.
#[derive(Clone)]
pub struct AppState {
    /// Read paths query the store directly; it shares its project cache
    /// with `ops`.
    pub store: Store,
    /// Every mutation goes through here (REST and `/mcp` alike).
    pub ops: Operations,
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
    /// DB id of the synthetic ai-agent admin (#189, resolved at startup).
    /// Extractors forge the agent identity with this id so `created_by`
    /// FKs point at a real user row.
    pub agent_user_id: std::sync::Arc<Option<String>>,
}

impl AppState {
    /// Build state with the production event subscribers wired into `ops`.
    pub fn new(
        store: Store,
        config: rungu_auth::AuthConfig,
        http_client: reqwest::Client,
        storage: Arc<dyn Storage>,
        branding: crate::meta::InstanceBranding,
        license: Arc<crate::meta::LicenseStatus>,
        agent_user_id: Arc<Option<String>>,
    ) -> Self {
        let events = default_event_bus(&store, &http_client);
        let ops = Operations::new(store.clone(), storage.clone(), events);
        Self { store, ops, config, http_client, storage, branding, license, agent_user_id }
    }
}

impl FromRef<AppState> for rungu_auth::AuthConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for rungu_auth::middleware::AgentUserId {
    fn from_ref(state: &AppState) -> Self {
        rungu_auth::middleware::AgentUserId((*state.agent_user_id).clone())
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
