//! Generic OpenID Connect login provider (for Gatehouse and any standards-
//! compliant issuer).
//!
//! `OIDC_ISSUER`, `OIDC_CLIENT_ID` and `OIDC_CLIENT_SECRET` turn on a fourth
//! login button beside Google, GitHub and Keycloak. The endpoints come from the
//! issuer's discovery document, fetched on first use and refreshed hourly, so a
//! restart is not needed when they move and an issuer that is down at boot does
//! not keep Rungu from starting (its button just fails until it is back).
//!
//! The flow is the same authorization-code flow as the other providers, plus
//! PKCE (S256): Gatehouse requires it for public clients and recommends it for
//! all, and it costs nothing here. Identity rules are unchanged: the userinfo
//! email must be explicitly verified, and the person is found or created by
//! email like on every other provider.

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rungu_auth::config::ProviderConfig;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;

const DISCOVERY_TTL: Duration = Duration::from_secs(3600);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DOC_BYTES: usize = 1_048_576;
/// Button label when `OIDC_NAME` is not set.
const DEFAULT_NAME: &str = "OpenID Connect";

/// The configured OIDC issuer for web login.
pub struct OidcLogin {
    name: String,
    issuer: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    http: reqwest::Client,
    cache: Mutex<Option<(Instant, ProviderConfig)>>,
}

impl OidcLogin {
    /// Read `OIDC_ISSUER`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET` and the
    /// optional `OIDC_NAME`. Unset issuer = disabled. A half-configured or
    /// malformed setting is a startup error: silently running without the
    /// login the operator asked for would be worse.
    pub fn from_env(app_url: &str, http: reqwest::Client) -> Result<Option<Arc<Self>>, String> {
        let var = |name: &str| std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let Some(issuer) = var("OIDC_ISSUER") else {
            return Ok(None);
        };
        let client_id = var("OIDC_CLIENT_ID").ok_or("OIDC_ISSUER is set but OIDC_CLIENT_ID is not")?;
        let client_secret = var("OIDC_CLIENT_SECRET").ok_or("OIDC_ISSUER is set but OIDC_CLIENT_SECRET is not")?;
        Self::new(&issuer, &client_id, &client_secret, var("OIDC_NAME").as_deref(), app_url, http)
            .map(|o| Some(Arc::new(o)))
    }

    pub fn new(
        issuer: &str,
        client_id: &str,
        client_secret: &str,
        name: Option<&str>,
        app_url: &str,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        let issuer = issuer.trim_end_matches('/').to_string();
        let url = Url::parse(&issuer).map_err(|e| format!("OIDC_ISSUER is not a URL: {e}"))?;
        crate::mcp_oauth::require_https(&url, "OIDC_ISSUER")?;
        Ok(Self {
            name: name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(DEFAULT_NAME).to_string(),
            issuer,
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: format!("{}/auth/oidc/callback", app_url.trim_end_matches('/')),
            http,
            cache: Mutex::default(),
        })
    }

    /// Label for the login button.
    pub fn label(&self) -> &str {
        &self.name
    }

    /// The provider config with the issuer's endpoints, discovered on first use.
    pub async fn provider(&self) -> Result<ProviderConfig, String> {
        if let Some((at, cfg)) = &*self.cache.lock().await
            && at.elapsed() < DISCOVERY_TTL
        {
            return Ok(cfg.clone());
        }
        let cfg = self.discover().await?;
        *self.cache.lock().await = Some((Instant::now(), cfg.clone()));
        Ok(cfg)
    }

    async fn discover(&self) -> Result<ProviderConfig, String> {
        let url = format!("{}/.well-known/openid-configuration", self.issuer);
        let mut response = self
            .http
            .get(&url)
            .timeout(FETCH_TIMEOUT)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("could not reach the OIDC issuer: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("OIDC discovery answered HTTP {}", response.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| format!("reading OIDC discovery: {e}"))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_DOC_BYTES {
                return Err("OIDC discovery document is too large".into());
            }
        }
        let doc: Value = serde_json::from_slice(&bytes).map_err(|e| format!("OIDC discovery is not JSON: {e}"))?;
        provider_from_discovery(&doc, &self.issuer, &self.client_id, &self.client_secret, &self.redirect_uri)
    }
}

/// Build the provider config from a discovery document, refusing one that does
/// not name the issuer it was fetched for, that lacks an endpoint Rungu needs,
/// that offers an endpoint over plain http, or that does not support PKCE S256.
fn provider_from_discovery(
    doc: &Value,
    issuer: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<ProviderConfig, String> {
    if doc.get("issuer").and_then(Value::as_str).map(|i| i.trim_end_matches('/')) != Some(issuer) {
        return Err("OIDC discovery names a different issuer than OIDC_ISSUER".into());
    }
    let endpoint = |field: &str| -> Result<String, String> {
        let value = doc.get(field).and_then(Value::as_str).ok_or_else(|| format!("OIDC discovery has no {field}"))?;
        let url = Url::parse(value).map_err(|_| format!("OIDC discovery {field} is not a URL"))?;
        crate::mcp_oauth::require_https(&url, field)?;
        Ok(value.to_string())
    };
    // When the issuer lists its PKCE methods, S256 must be one of them.
    if let Some(methods) = doc.get("code_challenge_methods_supported").and_then(Value::as_array)
        && !methods.iter().any(|m| m.as_str() == Some("S256"))
    {
        return Err("the OIDC issuer does not support PKCE S256".into());
    }
    Ok(ProviderConfig {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        redirect_uri: redirect_uri.to_string(),
        auth_url: endpoint("authorization_endpoint")?,
        token_url: endpoint("token_endpoint")?,
        userinfo_url: Some(endpoint("userinfo_endpoint")?),
    })
}

/// A fresh PKCE pair: the secret `code_verifier` kept in a cookie, and the
/// S256 `code_challenge` sent to the issuer.
pub fn new_pkce() -> (String, String) {
    // Two random UUIDs: 64 hex characters, within RFC 7636's 43..128 and its
    // unreserved-character set, with 244 random bits.
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = s256_challenge(&verifier);
    (verifier, challenge)
}

/// `BASE64URL(SHA256(verifier))` without padding (RFC 7636 §4.2).
pub fn s256_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn s256_matches_the_rfc_7636_example() {
        assert_eq!(
            s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_new_pkce_pair_is_valid_and_fresh() {
        let (verifier, challenge) = new_pkce();
        assert!((43..=128).contains(&verifier.len()));
        assert!(verifier.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_eq!(challenge, s256_challenge(&verifier));
        assert_ne!(new_pkce().0, verifier);
    }

    fn doc() -> Value {
        json!({
            "issuer": "https://id.example/api/auth",
            "authorization_endpoint": "https://id.example/api/auth/oauth2/authorize",
            "token_endpoint": "https://id.example/api/auth/oauth2/token",
            "userinfo_endpoint": "https://id.example/api/auth/oauth2/userinfo",
            "code_challenge_methods_supported": ["S256"],
        })
    }

    fn build(doc: &Value) -> Result<ProviderConfig, String> {
        provider_from_discovery(
            doc,
            "https://id.example/api/auth",
            "cid",
            "secret",
            "https://app.example/auth/oidc/callback",
        )
    }

    #[test]
    fn discovery_becomes_a_provider() {
        let cfg = build(&doc()).unwrap();
        assert_eq!(cfg.auth_url, "https://id.example/api/auth/oauth2/authorize");
        assert_eq!(cfg.token_url, "https://id.example/api/auth/oauth2/token");
        assert_eq!(cfg.userinfo_url.as_deref(), Some("https://id.example/api/auth/oauth2/userinfo"));
        assert_eq!(
            (cfg.client_id.as_str(), cfg.redirect_uri.as_str()),
            ("cid", "https://app.example/auth/oidc/callback")
        );
    }

    #[test]
    fn a_bad_discovery_document_is_refused() {
        let mut other_issuer = doc();
        other_issuer["issuer"] = json!("https://evil.example");
        assert!(build(&other_issuer).unwrap_err().contains("different issuer"));

        for field in ["authorization_endpoint", "token_endpoint", "userinfo_endpoint"] {
            let mut missing = doc();
            missing.as_object_mut().unwrap().remove(field);
            assert!(build(&missing).unwrap_err().contains(field), "{field}");

            let mut plain_http = doc();
            plain_http[field] = json!("http://id.example/x");
            assert!(build(&plain_http).unwrap_err().contains("https"), "{field}");
        }

        let mut no_s256 = doc();
        no_s256["code_challenge_methods_supported"] = json!(["plain"]);
        assert!(build(&no_s256).unwrap_err().contains("S256"));

        // An issuer that doesn't list its methods at all is given the benefit of the doubt.
        let mut unlisted = doc();
        unlisted.as_object_mut().unwrap().remove("code_challenge_methods_supported");
        assert!(build(&unlisted).is_ok());
    }

    #[test]
    fn settings_are_validated() {
        let http = reqwest::Client::new();
        let new =
            |issuer: &str| OidcLogin::new(issuer, "id", "secret", None, "https://app.example", http.clone()).err();
        assert!(new("http://id.example").unwrap().contains("https"));
        assert!(new("not a url").is_some());
        assert!(new("https://id.example/api/auth/").is_none());

        let login = OidcLogin::new(
            "https://id.example",
            "id",
            "s",
            Some("  CodeCora ID "),
            "https://app.example/",
            http.clone(),
        )
        .unwrap();
        assert_eq!(login.label(), "CodeCora ID");
        assert_eq!(login.redirect_uri, "https://app.example/auth/oidc/callback");
        let unnamed = OidcLogin::new("https://id.example", "id", "s", None, "https://app.example", http).unwrap();
        assert_eq!(unnamed.label(), "OpenID Connect");
    }
}
