//! HTTP server — Axum router, API routes, SPA handler, Swagger UI.

use axum::middleware::from_fn_with_state;
use axum::{Router, routing::get};
use rungu_api::AppState;
use rungu_api::openapi::ApiDoc;
use rungu_api::{api_routes, auth_routes};
// In-memory rate limiting — no external crate (governor was MSRV-incompatible).
use crate::ratelimit::{RateLimiter, rate_limit_middleware};
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use std::net::SocketAddr;
use std::time::Duration;

use crate::config::Config;
use crate::spa::spa_handler;

/// Build the Axum router and start serving.
pub async fn serve(config: Config, pool: sqlx::AnyPool, is_sqlite: bool, listen: &str) -> anyhow::Result<()> {
    let store = rungu_core::Store::new_with_kind(pool, is_sqlite);

    // Single shared HTTP client for outbound calls (OAuth token exchange, userinfo).
    // Reusing the client avoids per-request connection-pool and TLS setup.
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;

    // License validation for the white-label badge (#185). A network failure
    // or missing org id simply keeps the badge visible; never fatal. A valid
    // key hides the "Powered by Rungu" badge on all surfaces.
    let license = std::sync::Arc::new(rungu_api::meta::LicenseStatus::new());
    if let Some(key) = config.license_key.as_deref() {
        match config.license_org_id.as_deref() {
            None => tracing::warn!("RUNGU_LICENSE_KEY set but RUNGU_LICENSE_ORG_ID missing — badge stays visible"),
            Some(org) => match rungu_api::meta::validate_license(&http_client, key, org).await {
                Ok(info) => {
                    if info.licensed {
                        info!("License validated — Powered-by badge hidden");
                    } else {
                        tracing::warn!("License key not valid — Powered-by badge stays visible");
                    }
                    *license.0.write().await = Some(info);
                }
                Err(e) => tracing::warn!("License validation failed ({e}) — badge stays visible"),
            },
        }
    }

    let state = AppState {
        store,
        config: config.auth.clone(),
        http_client,
        storage: std::sync::Arc::from(rungu_core::create_storage()?),
        branding: config.branding.clone(),
        license,
    };

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

    // ── Rate limiters ───────────────────────────────────────────────────
    // Two independent per-IP limiters: a strict one for `/auth/*` (OAuth
    // login/callback abuse) and a looser one for `/api/*`. Both are `0`-able
    // via env to disable. Each limiter also spawns a pruner task so the IP
    // map can't grow unbounded.
    let api_routes = if config.rate_limit_per_min > 0 {
        let limiter = RateLimiter::new(config.rate_limit_per_min, Duration::from_secs(60), config.trust_proxy);
        limiter.spawn_pruner();
        tracing::info!("API rate limit: {} req/min per IP", config.rate_limit_per_min);
        api_routes().layer(from_fn_with_state(limiter, rate_limit_middleware))
    } else {
        tracing::info!("API rate limit: disabled (RUNGU_RATE_LIMIT_PER_MIN=0)");
        api_routes()
    };

    let auth_routes = if config.auth_rate_limit_per_min > 0 {
        let limiter = RateLimiter::new(config.auth_rate_limit_per_min, Duration::from_secs(60), config.trust_proxy);
        limiter.spawn_pruner();
        tracing::info!("Auth rate limit: {} req/min per IP", config.auth_rate_limit_per_min);
        auth_routes().layer(from_fn_with_state(limiter, rate_limit_middleware))
    } else {
        tracing::info!("Auth rate limit: disabled (RUNGU_AUTH_RATE_LIMIT_PER_MIN=0)");
        auth_routes()
    };

    let app = Router::new()
        .nest("/api", api_routes)
        .merge(auth_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check))
        .merge(crate::embed::router())
        // MCP over HTTP (#189) — root-mounted, Bearer-authed (same key as REST).
        .merge(rungu_api::mcp_http::mcp_routes())
        .fallback(spa_handler)
        // Sentry layer: capture HTTP request context and errors.
        // No-op when SENTRY_DSN is not set.
        .layer(sentry_tower::SentryHttpLayer::with_transaction())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen).await?;
    info!("Rungu listening on {listen}");
    info!("Swagger UI:  http://{listen}/swagger-ui");
    info!("OpenAPI spec: http://{listen}/api-docs/openapi.json");
    // `into_make_service_with_connect_info` exposes the peer `SocketAddr` to
    // the rate-limit middleware via `ConnectInfo<SocketAddr>`.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
