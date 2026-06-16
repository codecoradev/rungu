//! HTTP server — Axum router, API routes, SPA handler, Swagger UI.

use axum::{Router, routing::get};
use rungu_api::AppState;
use rungu_api::openapi::ApiDoc;
use rungu_api::{api_routes, auth_routes};
// Rate limiting removed — external crates require rustc >1.86.
// Will implement simple in-memory limiter in a future release.
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;
use crate::spa::spa_handler;

/// Build the Axum router and start serving.
pub async fn serve(config: Config, pool: sqlx::AnyPool, listen: &str) -> anyhow::Result<()> {
    let store = rungu_core::Store::new(pool);
    let state = AppState { store, config: config.auth.clone() };

    // CORS — secure by default.
    // If RUNGU_CORS_ORIGINS is empty, only allow the APP_URL origin.
    // To allow all origins (dev only), set RUNGU_CORS_ORIGINS=*.
    let cors = if config.cors_origins.iter().any(|o| o == "*") {
        // Explicit wildcard — dev mode only
        tracing::warn!("CORS set to allow all origins — not safe for production");
        CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
    } else if config.cors_origins.is_empty() {
        // Default: only allow APP_URL
        let app_origin: axum::http::HeaderValue = config
            .auth
            .app_url
            .parse()
            .unwrap_or_else(|_| "http://localhost:3000".parse().expect("valid header value"));
        CorsLayer::new().allow_origin(app_origin).allow_methods(Any).allow_headers(Any)
    } else {
        // Explicit origins from config
        CorsLayer::new()
            .allow_origin(config.cors_origins.iter().filter_map(|o| o.parse().ok()).collect::<Vec<_>>())
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let app = Router::new()
        .nest("/api", api_routes())
        .merge(auth_routes())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check))
        .fallback(spa_handler)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen).await?;
    info!("Rungu listening on {listen}");
    info!("Swagger UI:  http://{listen}/swagger-ui");
    info!("OpenAPI spec: http://{listen}/api-docs/openapi.json");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
