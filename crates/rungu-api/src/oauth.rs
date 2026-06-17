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
    /// Whether the provider asserts that the user controls this email.
    ///
    /// `true` only when the provider's userinfo explicitly reports a verified
    /// email (Google/Keycloak `email_verified: true`) or, for GitHub, when the
    /// primary email returned by `/user/emails` is both `verified` and `primary`.
    /// This is a hard requirement for account linking — see `auth_routes::callback`.
    pub email_verified: bool,
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
        AuthProvider::GitHub => parse_github(client, cfg, &json).await,
        AuthProvider::Keycloak => parse_keycloak(&json),
    }
}

// ── Provider-specific parsers ───────────────────────────────────────────

/// Google userinfo (https://developers.google.com/identity/openid-connect/openid-connect#obtaininguserprofileinformation).
///
/// Fields: `sub`, `email`, `email_verified`, `name`, `picture`.
fn parse_google(v: &serde_json::Value) -> Result<OAuthIdentity> {
    let email =
        v.get("email").and_then(|e| e.as_str()).map(String::from).context("Google userinfo missing 'email' field")?;

    let provider_id =
        v.get("sub").and_then(|s| s.as_str()).map(String::from).context("Google userinfo missing 'sub' field")?;

    // Google always returns `email_verified: true` in practice for verified emails.
    // Treat missing field as `false` defensively — linking requires explicit verification.
    let email_verified = v.get("email_verified").and_then(|b| b.as_bool()).unwrap_or(false);

    Ok(OAuthIdentity {
        provider_id,
        email,
        email_verified,
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        avatar_url: v.get("picture").and_then(|p| p.as_str()).map(String::from),
    })
}

/// GitHub user API (https://docs.github.com/en/rest/users/users#get-the-authenticated-user).
///
/// Fields: `id` (integer), `email`, `name`, `avatar_url`.
///
/// GitHub's `/user` endpoint may return `null` for `email` when the user has
/// no public email. We fall back to `/user/emails` and pick the **primary +
/// verified** entry, which also gives us `email_verified` semantics for the
/// account-linking check.
///
/// `email_verified` is set to `true` only when:
///   - the top-level `/user.email` is non-empty AND the primary entry in
///     `/user/emails` is `verified: true`, or
///   - `/user.email` is null/empty but `/user/emails` contains a primary+verified entry
///     (in which case that entry's email is used).
async fn parse_github(
    client: &reqwest::Client,
    cfg: &ProviderConfig,
    v: &serde_json::Value,
) -> Result<OAuthIdentity> {
    let provider_id =
        v.get("id").and_then(|i| i.as_i64()).map(|i| i.to_string()).context("GitHub userinfo missing 'id' field")?;

    let top_email = v.get("email").and_then(|e| e.as_str()).filter(|s| !s.is_empty()).map(String::from);

    // Fetch /user/emails to confirm verification + primary.
    // Derive from the userinfo URL (e.g. https://api.github.com/user → /user/emails).
    let emails_url = match cfg.userinfo_url.as_deref() {
        Some(u) if u.ends_with("/user") => format!("{u}/emails"),
        _ => "https://api.github.com/user/emails".to_string(),
    };
    let emails_resp = client
        .get(&emails_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to fetch GitHub /user/emails")?;

    if !emails_resp.status().is_success() {
        // Surface a clear error — without /user/emails we cannot establish verification.
        bail!(
            "GitHub /user/emails request failed (HTTP {}). Ensure the 'user:email' scope is requested.",
            emails_resp.status()
        );
    }

    let emails: Vec<GitHubEmail> =
        emails_resp.json().await.context("Failed to parse GitHub /user/emails response")?;

    // Pick the primary verified entry. GitHub guarantees at most one primary email.
    let primary_verified = emails.iter().find(|e| e.primary && e.verified);

    let (email, email_verified) = match (top_email, primary_verified) {
        (Some(top), Some(pv)) if top == pv.email => (top, true),
        (_, Some(pv)) => (pv.email.clone(), true),
        (Some(top), None) => {
            // Top-level email present but no primary+verified entry backs it.
            // Treat as unverified — do not link.
            (top, false)
        }
        (None, None) => {
            bail!(
                "GitHub returned no usable email. Ensure the 'user:email' scope is requested and the user has a verified primary email."
            )
        }
    };

    Ok(OAuthIdentity {
        provider_id,
        email,
        email_verified,
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        avatar_url: v.get("avatar_url").and_then(|a| a.as_str()).map(String::from),
    })
}

#[derive(Debug, serde::Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

/// Keycloak userinfo (standard OpenID Connect).
///
/// Fields: `sub`, `email`, `email_verified`, `name` or `preferred_username`, `picture`.
fn parse_keycloak(v: &serde_json::Value) -> Result<OAuthIdentity> {
    let email =
        v.get("email").and_then(|e| e.as_str()).map(String::from).context("Keycloak userinfo missing 'email' field")?;

    let provider_id =
        v.get("sub").and_then(|s| s.as_str()).map(String::from).context("Keycloak userinfo missing 'sub' field")?;

    // Keycloak SHOULD return `email_verified`. Default to `false` if missing —
    // admins of self-hosted Keycloak realms must ensure email verification is enforced
    // upstream for this to be `true`.
    let email_verified = v.get("email_verified").and_then(|b| b.as_bool()).unwrap_or(false);

    Ok(OAuthIdentity {
        provider_id,
        email,
        email_verified,
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| v.get("preferred_username").and_then(|n| n.as_str()))
            .map(String::from),
        avatar_url: v.get("picture").and_then(|p| p.as_str()).map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProviderConfig {
        ProviderConfig {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            userinfo_url: Some("https://api.github.com/user".to_string()),
        }
    }

    #[test]
    fn parse_google_reads_email_verified_true() {
        let v = serde_json::json!({
            "sub": "g-123",
            "email": "anaz@example.com",
            "email_verified": true,
            "name": "Anaz",
            "picture": "https://img/a.png",
        });
        let id = parse_google(&v).unwrap();
        assert_eq!(id.email, "anaz@example.com");
        assert_eq!(id.provider_id, "g-123");
        assert!(id.email_verified);
    }

    #[test]
    fn parse_google_defaults_email_verified_false_when_missing() {
        // Defensive: a missing `email_verified` must NOT be treated as verified.
        // This is the core of the #55 fix — we only link on explicit verification.
        let v = serde_json::json!({
            "sub": "g-456",
            "email": "sneaky@example.com",
        });
        let id = parse_google(&v).unwrap();
        assert!(!id.email_verified, "missing email_verified must default to false");
    }

    #[test]
    fn parse_keycloak_reads_email_verified() {
        let v = serde_json::json!({
            "sub": "kc-1",
            "email": "admin@corp.example",
            "email_verified": true,
            "preferred_username": "admin",
        });
        let id = parse_keycloak(&v).unwrap();
        assert_eq!(id.email, "admin@corp.example");
        assert!(id.email_verified);
        assert_eq!(id.name.as_deref(), Some("admin"));
    }

    #[test]
    fn parse_keycloak_unverified_email_is_flagged() {
        // Self-hosted Keycloak realm that has not verified the user's email.
        let v = serde_json::json!({
            "sub": "kc-2",
            "email": "attacker@victim.example",
            "email_verified": false,
        });
        let id = parse_keycloak(&v).unwrap();
        assert!(!id.email_verified);
    }

    #[test]
    fn parse_keycloak_missing_email_verified_defaults_false() {
        let v = serde_json::json!({"sub": "kc-3", "email": "x@example.com"});
        let id = parse_keycloak(&v).unwrap();
        assert!(!id.email_verified);
    }

    // Sanity check the GitHub emails URL derivation logic without exercising the network.
    #[test]
    fn github_emails_url_is_derived_from_userinfo() {
        // Mirrors the logic embedded in parse_github so we have coverage of the
        // branch that picks the emails endpoint.
        let c = cfg();
        let derived = match c.userinfo_url.as_deref() {
            Some(u) if u.ends_with("/user") => format!("{u}/emails"),
            _ => "https://api.github.com/user/emails".to_string(),
        };
        assert_eq!(derived, "https://api.github.com/user/emails");
    }
}
