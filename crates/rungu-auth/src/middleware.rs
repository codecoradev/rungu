//! Axum extractors — extract CurrentUser from JWT session cookie or API key.
//!
//! Two extractors:
//! - [`CurrentUser`] — rejects with 401 if not authenticated
//! - [`OptionalCurrentUser`] — never rejects, returns `user: None` if unauthenticated
//!
//! Machine access (#189): when `RUNGU_API_KEY` is configured, an
//! `Authorization: Bearer <key>` header authenticates as the synthetic
//! `ai-agent` admin user (`agent@rungu.local`). The key is compared in
//! constant time; a presented-but-wrong key is an immediate 401.
//!
//! ## State requirement
//!
//! These extractors are generic over any state `S` where
//! [`AuthConfig`](crate::AuthConfig) implements [`axum::extract::FromRef<S>`].
//! This avoids a circular dependency between `rungu-auth` and `rungu-api`.
//!
//! In `rungu-api`, implement `FromRef<AppState> for AuthConfig`.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use rungu_proto::CurrentUser as CurrentUserData;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::config::AuthConfig;
use crate::session::validate_jwt;

/// Session cookie name.
pub const SESSION_COOKIE: &str = "session";

/// Email of the synthetic admin user behind `RUNGU_API_KEY` machine access.
pub const AGENT_USER_EMAIL: &str = "agent@rungu.local";

/// Display name of the synthetic admin user.
pub const AGENT_USER_NAME: &str = "ai-agent";

/// Digest a key for constant-time comparison (key entropy >> digest size, so
/// hashing then comparing digests is equivalent to comparing the keys but
/// avoids length leaks and early-exit timing).
fn key_digest(key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.finalize().into()
}

/// Public constant-time API key check — used by the HTTP MCP transport
/// (`rungu-api::mcp_http`), which reads raw headers instead of extractors.
pub fn verify_api_key(config: &AuthConfig, presented: &str) -> bool {
    api_key_matches(config, presented)
}

/// Validate a presented API key against the configured one in constant time.
fn api_key_matches(config: &AuthConfig, presented: &str) -> bool {
    let Some(expected) = config.api_key.as_deref() else {
        return false;
    };
    if expected.is_empty() || presented.is_empty() {
        return false;
    }
    let a = key_digest(expected);
    let b = key_digest(presented);
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The synthetic identity behind a valid API key: full admin.
fn agent_identity() -> CurrentUserData {
    CurrentUserData {
        id: AGENT_USER_EMAIL.to_string(),
        email: AGENT_USER_EMAIL.to_string(),
        role: rungu_proto::UserRole::Admin,
    }
}

/// Extract a Bearer token from the `Authorization` header, if any.
fn extract_bearer(parts: &Parts) -> Option<String> {
    let header = parts.headers.get(axum::http::header::AUTHORIZATION)?;
    let value = header.to_str().ok()?;
    let rest = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    let token = rest.trim();
    if token.is_empty() { None } else { Some(token.to_string()) }
}

/// Extract current user from the `session` cookie.
///
/// Returns `401 Unauthorized` if:
/// - No session cookie is present
/// - The JWT is invalid or expired
///
/// # State requirement
///
/// Generic over any state `S` where `AuthConfig: FromRef<S>`.
#[derive(Debug)]
pub struct CurrentUser(pub CurrentUserData);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    AuthConfig: FromRef<S>,
{
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = AuthConfig::from_ref(state);

        // Machine access (#189). A presented-but-wrong key is an immediate
        // 401 — never fall through to cookie auth (prevents confusion attacks).
        if let Some(bearer) = extract_bearer(parts) {
            if api_key_matches(&config, &bearer) {
                return Ok(CurrentUser(agent_identity()));
            }
            warn!("Rejected request with invalid API key");
            return Err(axum::http::StatusCode::UNAUTHORIZED);
        }

        let token = extract_session_token(parts).ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

        match validate_jwt(&token, &config.app_secret) {
            Ok(user) => Ok(CurrentUser(user)),
            Err(e) => {
                warn!(error = %e, "Invalid or expired JWT session token");
                Err(axum::http::StatusCode::UNAUTHORIZED)
            }
        }
    }
}

/// Optional auth extractor — returns `None` if not authenticated (never rejects).
///
/// Use this for endpoints that behave differently for anonymous vs. authenticated
/// users (e.g., showing whether the current user has voted on a post).
#[derive(Debug)]
pub struct OptionalCurrentUser {
    pub user: Option<CurrentUserData>,
}

impl<S> FromRequestParts<S> for OptionalCurrentUser
where
    S: Send + Sync,
    AuthConfig: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = AuthConfig::from_ref(state);

        if let Some(bearer) = extract_bearer(parts) {
            if api_key_matches(&config, &bearer) {
                return Ok(Self { user: Some(agent_identity()) });
            }
            warn!("Rejected optional-auth request with invalid API key");
            return Ok(Self { user: None });
        }

        let Some(token) = extract_session_token(parts) else {
            return Ok(Self { user: None });
        };

        match validate_jwt(&token, &config.app_secret) {
            Ok(user) => Ok(Self { user: Some(user) }),
            Err(e) => {
                warn!(error = %e, "Invalid or expired JWT token in optional auth");
                Ok(Self { user: None })
            }
        }
    }
}

/// Extract the session token from the `session` cookie.
///
/// Reads the raw `Cookie` header from request parts (avoids needing to extract
/// `CookieJar` as a sub-extractor, which would require it in the state chain).
fn extract_session_token(parts: &Parts) -> Option<String> {
    let jar = CookieJar::from_headers(&parts.headers);
    jar.get(SESSION_COOKIE).map(|c| c.value().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthConfig;
    use crate::session::issue_jwt;
    use axum::body::Body;
    use axum::extract::FromRef;
    use axum::http::Request;
    use rungu_proto::{CurrentUser as CurrentUserData, UserRole};

    /// Test state — owns AuthConfig directly.
    #[derive(Clone)]
    struct TestState {
        auth: AuthConfig,
    }

    impl FromRef<TestState> for AuthConfig {
        fn from_ref(state: &TestState) -> Self {
            state.auth.clone()
        }
    }

    fn test_config() -> AuthConfig {
        AuthConfig {
            app_secret: "test-secret".to_string(),
            app_url: "http://localhost:3000".to_string(),
            secure_cookie: false,
            admin_emails: vec![],
            api_key: None,
            google: None,
            github: None,
            keycloak: None,
        }
    }

    fn test_user() -> CurrentUserData {
        CurrentUserData { id: "user-123".to_string(), email: "test@example.com".to_string(), role: UserRole::Member }
    }

    async fn extract_parts(req: Request<Body>) -> Parts {
        let (parts, _) = req.into_parts();
        parts
    }

    #[tokio::test]
    async fn test_current_user_valid_token() {
        let config = test_config();
        let user = test_user();
        let token = issue_jwt(&user, &config.app_secret).unwrap();

        let req = Request::builder().header("cookie", format!("session={token}")).body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = CurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert_eq!(extracted.0.id, "user-123");
        assert_eq!(extracted.0.email, "test@example.com");
    }

    #[tokio::test]
    async fn test_current_user_no_cookie_rejects_401() {
        let config = test_config();
        let req = Request::builder().body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = CurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_current_user_invalid_token_rejects_401() {
        let config = test_config();
        let req = Request::builder().header("cookie", "session=invalid.jwt.token").body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = CurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_optional_user_valid_token() {
        let config = test_config();
        let user = test_user();
        let token = issue_jwt(&user, &config.app_secret).unwrap();

        let req = Request::builder().header("cookie", format!("session={token}")).body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = OptionalCurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(extracted.user.is_some());
        assert_eq!(extracted.user.unwrap().id, "user-123");
    }

    #[tokio::test]
    async fn test_optional_user_no_cookie_returns_none() {
        let config = test_config();
        let req = Request::builder().body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = OptionalCurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_ok());
        assert!(result.unwrap().user.is_none());
    }

    #[tokio::test]
    async fn test_optional_user_invalid_token_returns_none() {
        let config = test_config();
        let req = Request::builder().header("cookie", "session=invalid.jwt.token").body(Body::empty()).unwrap();
        let mut parts = extract_parts(req).await;
        let state = TestState { auth: config };

        let result = OptionalCurrentUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_ok());
        assert!(result.unwrap().user.is_none());
    }
}
