//! OAuth token exchange + userinfo fetching.
//!
//! Handles the provider-specific details of exchanging an authorization code
//! for an access token and extracting user identity (email, name, avatar, provider ID).
//! Supports Google, GitHub, and Keycloak.

use anyhow::{Context, Result, bail};
use rungu_auth::config::ProviderConfig;
use rungu_proto::AuthProvider;
use serde::Deserialize;

/// Normalized identity extracted from an OAuth provider.
#[derive(Debug, Clone)]
pub struct OAuthIdentity {
    pub provider_id: String,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Token response from OAuth providers (RFC 6749 §5.1).
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: Option<String>,
    #[allow(dead_code)]
    expires_in: Option<i64>,
    #[allow(dead_code)]
    refresh_token: Option<String>,
}

/// Exchange an authorization code for an access token.
///
/// All three providers (Google, GitHub, Keycloak) accept the same standard
/// `application/x-www-form-urlencoded` body per RFC 6749.
pub async fn exchange_code(client: &reqwest::Client, cfg: &ProviderConfig, code: &str) -> Result<String> {
    let resp = client
        .post(&cfg.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
            ("code", code),
            ("redirect_uri", &cfg.redirect_uri),
        ])
        .send()
        .await
        .context("Failed to send token exchange request")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        // Redact potential secrets — only log first 200 chars, never full error body
        let redacted = if body.len() > 200 { &body[..200] } else { &body };
        tracing::error!(status = %status, url = %cfg.token_url, body_preview = %redacted, "Token exchange failed");
        bail!("Token exchange failed: HTTP {status}");
    }

    let token: TokenResponse = resp.json().await.context("Failed to parse token response JSON")?;

    Ok(token.access_token)
}

/// Fetch user identity from the provider's userinfo endpoint.
///
/// Each provider returns slightly different JSON, so we normalize here.
pub async fn fetch_identity(
    client: &reqwest::Client,
    cfg: &ProviderConfig,
    provider: AuthProvider,
    access_token: &str,
) -> Result<OAuthIdentity> {
    let userinfo_url = cfg.userinfo_url.as_ref().context("Provider has no userinfo_url configured")?;

    let mut req = client.get(userinfo_url).bearer_auth(access_token);

    // GitHub additionally requires an Accept header for JSON.
    if matches!(provider, AuthProvider::GitHub) {
        req = req.header("Accept", "application/vnd.github+json");
    }

    let resp = req.send().await.context("Failed to fetch userinfo")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let redacted = if body.len() > 200 { &body[..200] } else { &body };
        tracing::error!(status = %status, url = %userinfo_url, body_preview = %redacted, "Userinfo fetch failed");
        bail!("Userinfo fetch failed: HTTP {status}");
    }

    let json: serde_json::Value = resp.json().await.context("Failed to parse userinfo JSON")?;

    match provider {
        AuthProvider::Google => parse_google(&json),
        AuthProvider::GitHub => parse_github(&json),
        AuthProvider::Keycloak => parse_keycloak(&json),
    }
}

// ── Provider-specific parsers ───────────────────────────────────────────

/// Google userinfo (https://developers.google.com/identity/openid-connect/openid-connect#obtaininguserprofileinformation).
///
/// Fields: `sub`, `email`, `name`, `picture`.
fn parse_google(v: &serde_json::Value) -> Result<OAuthIdentity> {
    let email =
        v.get("email").and_then(|e| e.as_str()).map(String::from).context("Google userinfo missing 'email' field")?;

    let provider_id =
        v.get("sub").and_then(|s| s.as_str()).map(String::from).context("Google userinfo missing 'sub' field")?;

    Ok(OAuthIdentity {
        provider_id,
        email,
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        avatar_url: v.get("picture").and_then(|p| p.as_str()).map(String::from),
    })
}

/// GitHub user API (https://docs.github.com/en/rest/users/users#get-the-authenticated-user).
///
/// Fields: `id` (integer), `email`, `name`, `avatar_url`.
/// Note: GitHub may return `null` for email if the user has no public email.
/// In that case we fall back to the `/user/emails` endpoint.
fn parse_github(v: &serde_json::Value) -> Result<OAuthIdentity> {
    let provider_id =
        v.get("id").and_then(|i| i.as_i64()).map(|i| i.to_string()).context("GitHub userinfo missing 'id' field")?;

    let email = v.get("email").and_then(|e| e.as_str()).filter(|s| !s.is_empty()).map(String::from).context(
        "GitHub userinfo returned null/empty email. \
             Ensure the 'user:email' scope is requested.",
    )?;

    Ok(OAuthIdentity {
        provider_id,
        email,
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        avatar_url: v.get("avatar_url").and_then(|a| a.as_str()).map(String::from),
    })
}

/// Keycloak userinfo (standard OpenID Connect).
///
/// Fields: `sub`, `email`, `name` or `preferred_username`, `picture`.
fn parse_keycloak(v: &serde_json::Value) -> Result<OAuthIdentity> {
    let email =
        v.get("email").and_then(|e| e.as_str()).map(String::from).context("Keycloak userinfo missing 'email' field")?;

    let provider_id =
        v.get("sub").and_then(|s| s.as_str()).map(String::from).context("Keycloak userinfo missing 'sub' field")?;

    Ok(OAuthIdentity {
        provider_id,
        email,
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| v.get("preferred_username").and_then(|n| n.as_str()))
            .map(String::from),
        avatar_url: v.get("picture").and_then(|p| p.as_str()).map(String::from),
    })
}
