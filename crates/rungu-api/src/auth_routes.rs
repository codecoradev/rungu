//! Auth routes — login, callback, logout, me, providers.

use axum::{Json, Router, routing::get};

use crate::AppState;

/// Build auth routes.
pub fn auth_routes() -> Router<AppState> {
    Router::new().route("/auth/providers", get(list_providers))
}

async fn list_providers(axum::extract::State(state): axum::extract::State<AppState>) -> Json<serde_json::Value> {
    let providers = state.config.active_providers();
    Json(serde_json::json!({ "providers": providers }))
}
