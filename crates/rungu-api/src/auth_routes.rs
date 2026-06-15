//! Auth routes — login, callback, logout, me, providers.

use axum::{Json, Router, routing::get};

/// Build auth routes.
pub fn auth_routes() -> Router {
    Router::new()
        .route("/auth/providers", get(list_providers))
        // OAuth flows — to be implemented
        // .route("/auth/google/login", get(google_login))
        // .route("/auth/google/callback", get(google_callback))
        // .route("/auth/github/login", get(github_login))
        // .route("/auth/github/callback", get(github_callback))
        // .route("/auth/keycloak/login", get(keycloak_login))
        // .route("/auth/keycloak/callback", get(keycloak_callback))
        // .route("/auth/logout", get(logout))
        // .route("/auth/me", get(me))
}

async fn list_providers() -> Json<serde_json::Value> {
    // TODO: return active providers from AuthConfig
    Json(serde_json::json!({ "providers": [] }))
}
