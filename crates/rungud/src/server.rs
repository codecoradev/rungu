//! HTTP server — Axum router, API routes, SPA handler.

use axum::{Router, middleware, routing::get};
use rungu_core::Store;
use rungu_api::auth_routes;
use sqlx::SqlitePool;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::Config;
use crate::spa::spa_handler;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: Config,
}

/// Build the Axum router and start serving.
pub async fn serve(config: Config, pool: SqlitePool, listen: &str) -> anyhow::Result<()> {
    let store = Store::new(pool);
    let state = AppState {
        store: store.clone(),
        config: config.clone(),
    };

    let api_routes = Router::new()
        // Auth (public)
        .merge(auth_routes());

    // CORS
    let cors = if config.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(
                config
                    .cors_origins
                    .iter()
                    .filter_map(|o| o.parse().ok())
                    .collect::<Vec<_>>(),
            )
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let app = Router::new()
        .nest("/api", api_routes)
        .route("/health", get(health_check))
        .fallback(spa_handler)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen).await?;
    info!("Rungu listening on {listen}");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
