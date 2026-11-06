//! HTTP server — Axum router, API routes, SPA handler, Swagger UI.

use axum::middleware::from_fn_with_state;
use axum::{Router, routing::get};
use rungu_api::AppState;
use rungu_api::openapi::ApiDoc;
use rungu_api::{api_routes, auth_flow_routes, auth_session_routes};
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
    // Machine access bootstrap (#189): resolve the synthetic ai-agent admin
    // when RUNGU_API_KEY is set. Its DB id feeds the auth extractors so
    // agent-created rows carry a valid created_by FK.
    let agent_user_id: std::sync::Arc<Option<String>> = if config.auth.api_key.is_some() {
        let bootstrap = rungu_core::Store::new_with_kind(pool.clone(), is_sqlite);
        let agent = bootstrap
            .find_or_create_user(
                rungu_auth::middleware::AGENT_USER_EMAIL,
                Some(rungu_auth::middleware::AGENT_USER_NAME),
                None,
                &[rungu_auth::middleware::AGENT_USER_EMAIL.to_string()],
            )
            .await?;
        info!("Machine access enabled: API key active as {} ({})", agent.email, agent.id);
        std::sync::Arc::new(Some(agent.id))
    } else {
        std::sync::Arc::new(None)
    };

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

    let mcp = rungu_api::mcp_oauth::McpSettings::from_env(
        &config.auth.app_url,
        &config.auth.admin_emails,
        http_client.clone(),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    if let Some(oauth) = &mcp.oauth {
        info!("MCP OAuth: accepting tokens from {} for {}", oauth.issuer(), oauth.resource());
    }

    let email = rungu_api::email::EmailConfig::from_env();
    info!("Email notifications: {}", email.describe());
    let state = AppState::new(
        store,
        config.auth.clone(),
        http_client,
        std::sync::Arc::from(rungu_core::create_storage()?),
        config.branding.clone(),
        license,
        agent_user_id,
    )
    .with_email(email)
    .with_mcp(mcp);

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

    let routes = rate_limited_routes(config.rate_limit_per_min, config.auth_rate_limit_per_min, config.trust_proxy);

    let app = Router::new()
        .nest("/api", routes.api)
        .merge(routes.auth_flow)
        .merge(routes.auth_session)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check))
        .merge(crate::embed::router())
        .fallback(spa_handler)
        // Sentry layer: capture HTTP request context and errors.
        // No-op when SENTRY_DSN is not set.
        .layer(sentry_tower::SentryHttpLayer::with_transaction())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // MCP over HTTP (#189) and its OAuth discovery documents sit OUTSIDE the
        // site-wide CORS layer: they answer their own preflights and allowed
        // origins (claude.ai) — see `mcp_http`.
        .merge(rungu_api::mcp_http::mcp_routes().layer(TraceLayer::new_for_http()))
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

/// Route groups with their rate limiters applied.
pub(crate) struct RateLimitedRoutes {
    /// `/api/*` routes (mount with `.nest("/api", ..)`).
    pub api: Router<AppState>,
    /// `/auth/me`, `/auth/providers`, `/auth/logout`.
    pub auth_session: Router<AppState>,
    /// `/auth/{provider}/login`, `/auth/{provider}/callback`.
    pub auth_flow: Router<AppState>,
}

/// Wire the two per-IP limiters onto their route groups.
///
/// A strict limiter (`auth_per_min`) guards the OAuth flow, where login and
/// callback abuse happens. A looser one (`api_per_min`) is shared by `/api/*`
/// and the session routes the SPA hits on ordinary page loads: behind the
/// strict limiter, `/auth/me` returned 429 after about 30 page loads a minute
/// and the UI showed signed-in users as logged out. `0` disables a limiter.
/// Each limiter spawns a pruner task so its IP map can't grow unbounded.
pub(crate) fn rate_limited_routes(api_per_min: u32, auth_per_min: u32, trust_proxy: bool) -> RateLimitedRoutes {
    let (api, auth_session) = if api_per_min > 0 {
        let limiter = RateLimiter::new(api_per_min, Duration::from_secs(60), trust_proxy);
        limiter.spawn_pruner();
        tracing::info!("API rate limit: {api_per_min} req/min per IP");
        (
            api_routes().layer(from_fn_with_state(limiter.clone(), rate_limit_middleware)),
            auth_session_routes().layer(from_fn_with_state(limiter, rate_limit_middleware)),
        )
    } else {
        tracing::info!("API rate limit: disabled (RUNGU_RATE_LIMIT_PER_MIN=0)");
        (api_routes(), auth_session_routes())
    };

    let auth_flow = if auth_per_min > 0 {
        let limiter = RateLimiter::new(auth_per_min, Duration::from_secs(60), trust_proxy);
        limiter.spawn_pruner();
        tracing::info!("Auth rate limit: {auth_per_min} req/min per IP");
        auth_flow_routes().layer(from_fn_with_state(limiter, rate_limit_middleware))
    } else {
        tracing::info!("Auth rate limit: disabled (RUNGU_AUTH_RATE_LIMIT_PER_MIN=0)");
        auth_flow_routes()
    };

    RateLimitedRoutes { api, auth_session, auth_flow }
}

async fn health_check() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_state() -> AppState {
        let pool = rungu_core::open_pool("sqlite::memory:").await.unwrap();
        rungu_core::run_migrations(&pool, "sqlite::memory:").await.unwrap();
        let config = rungu_auth::AuthConfig {
            app_secret: "test-secret".to_string(),
            app_url: "http://localhost:3000".to_string(),
            secure_cookie: false,
            admin_emails: vec![],
            api_key: None,
            google: None,
            github: None,
            keycloak: None,
        };
        AppState::new(
            rungu_core::Store::new_with_kind(pool, true),
            config,
            reqwest::Client::new(),
            std::sync::Arc::from(
                rungu_core::FsStorage::new(std::env::temp_dir().join("rungud-ratelimit-test")).unwrap(),
            ),
            rungu_api::meta::InstanceBranding::default(),
            std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
            std::sync::Arc::new(None),
        )
    }

    async fn status(app: &Router, path: &str) -> StatusCode {
        let mut req = Request::builder().uri(path).body(Body::empty()).unwrap();
        // The limiter keys on the peer address, which axum::serve normally injects.
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 4000))));
        app.clone().oneshot(req).await.unwrap().status()
    }

    /// Regression: `/auth/me` runs on every page load and must not share the
    /// strict OAuth budget, or signed-in users appear logged out.
    #[tokio::test]
    async fn session_routes_use_the_api_budget_not_the_auth_budget() {
        let routes = rate_limited_routes(100, 3, false);
        let app: Router = Router::new()
            .nest("/api", routes.api)
            .merge(routes.auth_flow)
            .merge(routes.auth_session)
            .with_state(test_state().await);

        // Well past the strict budget of 3: never rate limited (401 = no session).
        for _ in 0..10 {
            assert_eq!(status(&app, "/auth/me").await, StatusCode::UNAUTHORIZED);
            assert_ne!(status(&app, "/auth/providers").await, StatusCode::TOO_MANY_REQUESTS);
        }

        // The OAuth flow keeps the strict budget: the 4th login is rejected.
        for _ in 0..3 {
            assert_ne!(status(&app, "/auth/github/login").await, StatusCode::TOO_MANY_REQUESTS);
        }
        assert_eq!(status(&app, "/auth/github/login").await, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn zero_disables_both_limiters() {
        let routes = rate_limited_routes(0, 0, false);
        let app: Router =
            Router::new().merge(routes.auth_flow).merge(routes.auth_session).with_state(test_state().await);
        for _ in 0..10 {
            assert_ne!(status(&app, "/auth/github/login").await, StatusCode::TOO_MANY_REQUESTS);
            assert_ne!(status(&app, "/auth/me").await, StatusCode::TOO_MANY_REQUESTS);
        }
    }
}
