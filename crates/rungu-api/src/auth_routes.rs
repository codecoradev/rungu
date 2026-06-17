//! Auth routes — login, callback, logout, me, providers.
//!
//! Full OAuth2 Authorization Code flow with CSRF protection.
//! Supports Google, GitHub, and Keycloak providers.

use axum::extract::{Path, Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::AppendHeaders;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::session;
use rungu_proto::AuthProvider;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::AppState;
use crate::oauth;

// ── Cookie constants ────────────────────────────────────────────────────

/// Session cookie name — holds the JWT.
const SESSION_COOKIE: &str = "session";

/// CSRF state cookie name — validates the OAuth round-trip.
const STATE_COOKIE: &str = "oauth_state";

/// Session cookie max-age: 7 days (matches JWT expiry).
const SESSION_MAX_AGE_SECS: i64 = 7 * 24 * 60 * 60;

/// CSRF state cookie max-age: 10 minutes.
const STATE_MAX_AGE_SECS: i64 = 10 * 60;

// ── Route builder ───────────────────────────────────────────────────────

/// Build auth routes.
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/providers", axum::routing::get(list_providers))
        .route("/auth/{provider}/login", axum::routing::get(login))
        .route("/auth/{provider}/callback", axum::routing::get(callback))
        .route("/auth/logout", axum::routing::post(logout))
        .route("/auth/me", axum::routing::get(me))
}

// ── Handlers ────────────────────────────────────────────────────────────

/// `GET /auth/providers` — list active providers for login buttons.
async fn list_providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    let providers = state.config.active_providers();
    Json(serde_json::json!({ "providers": providers }))
}

/// Query params for the login endpoint.
#[derive(Debug, Deserialize)]
struct LoginQuery {
    /// Optional redirect path after successful auth (default: "/").
    redirect: Option<String>,
}

/// `GET /auth/:provider/login` — redirect to provider's authorization URL.
///
/// Generates a random CSRF state token, stores it in a short-lived HttpOnly cookie,
/// and redirects (302) the user to the OAuth provider's consent screen.
async fn login(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<LoginQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let (cfg, provider_enum) = state.config.get_provider(&provider).ok_or(StatusCode::NOT_FOUND)?;

    // Generate CSRF state token
    let state_token = uuid::Uuid::new_v4().to_string();

    // Build authorization URL
    let (scope, auth_params) = provider_scopes(provider_enum);
    let mut all_params = auth_params;
    all_params.insert("client_id".to_string(), cfg.client_id.clone());
    all_params.insert("redirect_uri".to_string(), cfg.redirect_uri.clone());
    all_params.insert("response_type".to_string(), "code".to_string());
    all_params.insert("state".to_string(), state_token.clone());

    // Store redirect target in state — validate to prevent open redirect.
    // Only allow relative paths starting with "/" (no protocol-relative "//").
    if let Some(ref r) = query.redirect {
        if r.starts_with('/') && !r.starts_with("//") && !r.contains("\\") {
            all_params.insert("redirect_target".to_string(), r.clone());
        } else {
            tracing::warn!(redirect = %r, "Rejecting invalid redirect target");
        }
    }

    // Serialize query params manually (HashMap<String,String> → key=val&...)
    let query_string = all_params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let auth_url = format!("{}?{}", cfg.auth_url, query_string);
    let _ = scope; // scope already merged into params

    // Build state cookie (short-lived, HttpOnly)
    let state_cookie = build_cookie(STATE_COOKIE, &state_token, STATE_MAX_AGE_SECS, state.config.secure_cookie);

    info!(provider = %provider, "OAuth login initiated");

    Ok((StatusCode::FOUND, [(axum::http::header::LOCATION, auth_url)], [(SET_COOKIE, state_cookie)]))
}

/// Query params for the OAuth callback.
#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    /// Our custom param echoed back from the provider — but since OAuth providers
    /// don't echo arbitrary params, we instead store redirect in the session.
    /// This field is for future use / custom flows.
    #[allow(dead_code)]
    redirect_target: Option<String>,
}

/// `GET /auth/:provider/callback` — handle OAuth provider callback.
///
/// Validates state, exchanges code for access token, fetches user identity,
/// creates/finds user in DB, issues JWT, sets session cookie, redirects to "/".
async fn callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check for OAuth error from provider
    if let Some(err) = &query.error {
        warn!(provider = %provider, error = %err, "OAuth provider returned error");
        return Err((StatusCode::BAD_REQUEST, format!("OAuth error: {err}")));
    }

    let code = query.code.as_deref().ok_or((StatusCode::BAD_REQUEST, "Missing 'code' parameter".to_string()))?;

    let state_token =
        query.state.as_deref().ok_or((StatusCode::BAD_REQUEST, "Missing 'state' parameter".to_string()))?;

    // Validate state against cookie
    let cookie_state = extract_cookie_value(&headers, STATE_COOKIE)
        .ok_or((StatusCode::BAD_REQUEST, "Missing or invalid state cookie".to_string()))?;

    if state_token != cookie_state {
        warn!("OAuth state mismatch — possible CSRF attempt");
        return Err((StatusCode::BAD_REQUEST, "State mismatch".to_string()));
    }

    // Resolve provider config
    let (cfg, provider_enum) = state
        .config
        .get_provider(&provider)
        .ok_or((StatusCode::NOT_FOUND, format!("Provider '{provider}' not configured")))?;

    // Exchange code → access token → fetch identity
    let http_client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| {
        error!(error = %e, "Failed to build HTTP client");
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".to_string())
    })?;

    let access_token = oauth::exchange_code(&http_client, cfg, code).await.map_err(|e| {
        error!(error = %e, "Token exchange failed");
        (StatusCode::BAD_GATEWAY, format!("Token exchange failed: {e}"))
    })?;

    let identity = oauth::fetch_identity(&http_client, cfg, provider_enum, &access_token).await.map_err(|e| {
        error!(error = %e, "Userinfo fetch failed");
        (StatusCode::BAD_GATEWAY, format!("Failed to fetch user info: {e}"))
    })?;

    info!(provider = %provider, email = %identity.email, verified = identity.email_verified, "OAuth identity fetched");

    // Hard gate: only link accounts when the provider asserts email ownership.
    // This blocks cross-provider account takeover via unverified-email providers
    // (e.g. attacker-controlled Keycloak realm, self-hosted IdP with admin-reassignable emails).
    // See docs/auth/overview.md and issue #55.
    if !identity.email_verified {
        warn!(provider = %provider, email = %identity.email, "Rejecting login: email not verified by provider");
        return Err((
            StatusCode::FORBIDDEN,
            "Email is not verified by the provider. Verify your email with the provider and try again.".to_string(),
        ));
    }

    // Find or create user (email dedup)
    let user = state
        .store
        .find_or_create_user(
            &identity.email,
            identity.name.as_deref(),
            identity.avatar_url.as_deref(),
            &state.config.admin_emails,
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to find/create user");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?;

    // Link identity
    let provider_name = provider_enum_as_str(provider_enum);
    if let Err(e) = state.store.upsert_identity(&user.id, provider_name, &identity.provider_id).await {
        // Non-fatal — user is created, identity link can retry next login.
        warn!(error = %e, "Failed to upsert user identity (non-fatal)");
    }

    // Issue JWT + set session cookie
    let current_user = rungu_proto::CurrentUser { id: user.id.clone(), email: user.email.clone(), role: user.role };

    let jwt = session::issue_jwt(&current_user, &state.config.app_secret).map_err(|e| {
        error!(error = %e, "Failed to issue JWT");
        (StatusCode::INTERNAL_SERVER_ERROR, "Session error".to_string())
    })?;

    let session_cookie = build_cookie(SESSION_COOKIE, &jwt, SESSION_MAX_AGE_SECS, state.config.secure_cookie);
    let clear_state_cookie = clear_cookie(STATE_COOKIE, state.config.secure_cookie);

    info!(user_id = %user.id, provider = %provider, "OAuth login successful");

    Ok((
        StatusCode::FOUND,
        [(axum::http::header::LOCATION, "/".to_string())],
        AppendHeaders([(SET_COOKIE, session_cookie), (SET_COOKIE, clear_state_cookie)]),
    ))
}

/// `POST /auth/logout` — clear session cookie.
///
/// Returns 200 with cleared cookie. Client should redirect to "/".
async fn logout(State(state): State<AppState>) -> impl IntoResponse {
    let clear_session = clear_cookie(SESSION_COOKIE, state.config.secure_cookie);

    info!("User logout");

    (StatusCode::OK, [(SET_COOKIE, clear_session)], Json(serde_json::json!({ "ok": true })))
}

/// `GET /auth/me` — return current user info from JWT session.
///
/// Returns 401 if not authenticated.
async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    let token = extract_cookie_value(&headers, SESSION_COOKIE).ok_or(StatusCode::UNAUTHORIZED)?;

    let current_user = session::validate_jwt(&token, &state.config.app_secret).map_err(|e| {
        warn!(error = %e, "Invalid JWT on /auth/me");
        StatusCode::UNAUTHORIZED
    })?;

    Ok(Json(serde_json::json!({
        "data": {
            "id": current_user.id,
            "email": current_user.email,
            "role": format!("{:?}", current_user.role).to_lowercase(),
        }
    })))
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Build a Set-Cookie value string.
/// Uses SameSite=None for OAuth compatibility (cross-site redirect from provider).
/// `secure` controls whether the Secure flag is included (false for local HTTP).
fn build_cookie(name: &str, value: &str, max_age_secs: i64, secure: bool) -> String {
    // SameSite=None requires Secure in production. For local HTTP, omit both None and Secure
    // so the cookie defaults to SameSite=Lax (browsers allow this on localhost redirects).
    if secure {
        format!("{name}={value}; Path=/; HttpOnly; Secure; SameSite=None; Max-Age={max_age_secs}")
    } else {
        format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}")
    }
}

/// Build a Set-Cookie value that immediately expires the cookie.
fn clear_cookie(name: &str, secure: bool) -> String {
    if secure {
        format!("{name}=; Path=/; HttpOnly; Secure; SameSite=None; Max-Age=0")
    } else {
        format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    }
}

/// Extract a cookie value from the Cookie header.
fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Get provider-specific OAuth scopes.
fn provider_scopes(provider: AuthProvider) -> (&'static str, HashMap<String, String>) {
    let scope = match provider {
        AuthProvider::Google => "openid email profile",
        AuthProvider::GitHub => "read:user user:email",
        AuthProvider::Keycloak => "openid email profile",
    };

    let mut params = HashMap::new();
    params.insert("scope".to_string(), scope.to_string());
    (scope, params)
}

/// Convert AuthProvider enum to the string used in the DB.
fn provider_enum_as_str(provider: AuthProvider) -> &'static str {
    match provider {
        AuthProvider::Google => "google",
        AuthProvider::GitHub => "github",
        AuthProvider::Keycloak => "keycloak",
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cookie_secure() {
        let c = build_cookie("session", "abc123", 604800, true);
        assert!(c.contains("session=abc123"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("Secure"));
        assert!(c.contains("Max-Age=604800"));
    }

    #[test]
    fn test_build_cookie_insecure() {
        // Local HTTP dev mode — no Secure flag
        let c = build_cookie("session", "abc123", 604800, false);
        assert!(c.contains("session=abc123"));
        assert!(c.contains("HttpOnly"));
        assert!(!c.contains("Secure"));
    }

    #[test]
    fn test_clear_cookie() {
        let c = clear_cookie("session", true);
        assert!(c.contains("session=;"));
        assert!(c.contains("Max-Age=0"));
    }

    #[test]
    fn test_extract_cookie_value() {
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::COOKIE, "session=eyJ.abc; oauth_state=xyz-123".parse().unwrap());

        assert_eq!(extract_cookie_value(&headers, "session"), Some("eyJ.abc".to_string()));
        assert_eq!(extract_cookie_value(&headers, "oauth_state"), Some("xyz-123".to_string()));
        assert_eq!(extract_cookie_value(&headers, "nonexistent"), None);
    }

    #[test]
    fn test_provider_enum_as_str() {
        assert_eq!(provider_enum_as_str(AuthProvider::Google), "google");
        assert_eq!(provider_enum_as_str(AuthProvider::GitHub), "github");
        assert_eq!(provider_enum_as_str(AuthProvider::Keycloak), "keycloak");
    }
}
