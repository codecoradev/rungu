//! OAuth resource-server mode for `POST /mcp`.
//!
//! With `RUNGU_MCP_OAUTH_ISSUER` set, `/mcp` also accepts an OAuth access
//! token (a JWT) issued by that authorization server, so remote MCP clients
//! such as Claude connectors can sign in as a person instead of sharing the
//! API key. Rungu stays a pure *resource server*: it never issues tokens, it
//! advertises its issuer (RFC 9728 protected-resource metadata), verifies
//! tokens locally against the issuer's JWKS, and maps the token's subject to
//! a Rungu user through the same email-based identity as the web login.
//!
//! The static API key keeps working beside it.
//!
//! Token rules: signature by a key from the issuer's JWKS, `iss` equal to the
//! issuer, `aud` containing the exact MCP URL, `exp`/`nbf` honoured, `sub`
//! present. Sender-constrained tokens (`cnf`: DPoP / mTLS) and opaque tokens
//! are rejected — Rungu can't verify proof of possession. The role comes from
//! `ADMIN_EMAILS` like any other login, never from the token.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rungu_core::{Actor, Store};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;
use url::Url;

/// Browser origins allowed to call `/mcp` without configuration.
const BUILTIN_ORIGINS: &[&str] = &["https://claude.ai", "https://claude.com"];
/// Signing algorithms accepted. Symmetric and `none` are never allowed.
const ALGORITHMS: &[Algorithm] =
    &[Algorithm::RS256, Algorithm::RS384, Algorithm::RS512, Algorithm::ES256, Algorithm::ES384];

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DOC_BYTES: usize = 1_048_576;
const DISCOVERY_TTL: Duration = Duration::from_secs(3600);
const JWKS_TTL: Duration = Duration::from_secs(600);
/// Never refetch the JWKS more often than this, whatever `kid`s arrive.
const JWKS_MIN_REFETCH: Duration = Duration::from_secs(30);
/// A verified subject keeps its resolved actor this long (saves the userinfo
/// call and a database write per request). The token is still verified every time.
const ACTOR_TTL: Duration = Duration::from_secs(60);
const MAX_CACHED_ACTORS: usize = 1024;
/// After a failed discovery fetch, answer "unavailable" for this long.
const DISCOVERY_BACKOFF: Duration = Duration::from_secs(10);

/// Why a bearer token was refused. Logged by name, never echoed with the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reject {
    /// Not a JWT (opaque tokens can't be verified locally).
    Opaque,
    UnsupportedAlgorithm,
    UnknownKey,
    /// Signature, issuer, audience or expiry check failed.
    Invalid(&'static str),
    /// Sender-constrained token (`cnf`).
    SenderConstrained,
    NoSubject,
    /// The token carries no email and userinfo gave none.
    NoEmail,
    UnverifiedEmail,
    /// The issuer's metadata, keys or userinfo couldn't be fetched.
    IssuerUnavailable,
}

impl Reject {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opaque => "opaque_token",
            Self::UnsupportedAlgorithm => "unsupported_algorithm",
            Self::UnknownKey => "unknown_key",
            Self::Invalid(why) => why,
            Self::SenderConstrained => "sender_constrained_token",
            Self::NoSubject => "no_subject",
            Self::NoEmail => "no_email",
            Self::UnverifiedEmail => "unverified_email",
            Self::IssuerUnavailable => "issuer_unavailable",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    cnf: Option<Value>,
}

#[derive(Clone)]
struct Discovery {
    jwks_uri: String,
    userinfo_endpoint: Option<String>,
    raw: Value,
}

#[derive(Default)]
struct Cache {
    discovery: Option<(Instant, Discovery)>,
    /// When discovery last failed; failures back off instead of retrying per request.
    discovery_failed: Option<Instant>,
    jwks: Option<(Instant, JwkSet)>,
    /// When the JWKS was last *attempted* (success or not).
    jwks_fetched: Option<Instant>,
    /// Resolved actor per subject, with the email it was resolved for.
    actors: HashMap<String, (Instant, Actor, String)>,
}

/// An OAuth issuer Rungu trusts for `/mcp`.
pub struct McpOAuth {
    issuer: String,
    resource: Url,
    http: reqwest::Client,
    admin_emails: Vec<String>,
    /// Accept a token whose email carries no `email_verified` claim at all.
    allow_unverified_email: bool,
    cache: Mutex<Cache>,
    /// Serialises issuer refreshes so a burst of requests fetches once.
    refresh: Mutex<()>,
}

/// What `/mcp` needs beyond the API key: the optional OAuth issuer and any
/// extra browser origins. Cheap to clone.
#[derive(Clone, Default)]
pub struct McpSettings {
    pub oauth: Option<Arc<McpOAuth>>,
    pub extra_origins: Vec<String>,
}

impl McpSettings {
    /// Read `RUNGU_MCP_OAUTH_ISSUER`, `RUNGU_MCP_RESOURCE_URL` (default
    /// `<APP_URL>/mcp`) and `RUNGU_MCP_ALLOWED_ORIGINS`. A bad value is a
    /// startup error: running with a half-configured issuer would silently
    /// fall back to API-key-only.
    pub fn from_env(app_url: &str, admin_emails: &[String], http: reqwest::Client) -> Result<Self, String> {
        let flag =
            |name: &str| matches!(std::env::var(name).ok().as_deref().map(str::trim), Some("1" | "true" | "yes"));
        let var = |name: &str| std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let extra_origins = var("RUNGU_MCP_ALLOWED_ORIGINS")
            .map(|v| {
                v.split(',').map(|o| o.trim().trim_end_matches('/').to_string()).filter(|o| !o.is_empty()).collect()
            })
            .unwrap_or_default();
        let oauth = match var("RUNGU_MCP_OAUTH_ISSUER") {
            None => None,
            Some(issuer) => {
                let resource =
                    var("RUNGU_MCP_RESOURCE_URL").unwrap_or_else(|| format!("{}/mcp", app_url.trim_end_matches('/')));
                let mut oauth = McpOAuth::new(&issuer, &resource, admin_emails.to_vec(), http)?;
                oauth.allow_unverified_email = flag("RUNGU_MCP_OAUTH_ALLOW_UNVERIFIED_EMAIL");
                Some(Arc::new(oauth))
            }
        };
        Ok(Self { oauth, extra_origins })
    }

    /// Whether a browser `Origin` may call `/mcp`.
    pub fn origin_allowed(&self, origin: &str, app_url: &str) -> bool {
        let origin = origin.trim_end_matches('/');
        let own = |url: &str| Url::parse(url).ok().map(|u| u.origin().ascii_serialization());
        BUILTIN_ORIGINS.contains(&origin)
            || self.extra_origins.iter().any(|o| o == origin)
            || own(app_url).as_deref() == Some(origin)
            || self.oauth.as_ref().and_then(|o| own(o.resource.as_str())).as_deref() == Some(origin)
    }
}

impl McpOAuth {
    pub fn new(issuer: &str, resource: &str, admin_emails: Vec<String>, http: reqwest::Client) -> Result<Self, String> {
        let issuer = issuer.trim_end_matches('/').to_string();
        let issuer_url = Url::parse(&issuer).map_err(|e| format!("RUNGU_MCP_OAUTH_ISSUER is not a URL: {e}"))?;
        require_https(&issuer_url, "RUNGU_MCP_OAUTH_ISSUER")?;
        let resource = Url::parse(resource).map_err(|e| format!("RUNGU_MCP_RESOURCE_URL is not a URL: {e}"))?;
        require_https(&resource, "RUNGU_MCP_RESOURCE_URL")?;
        if resource.query().is_some() || resource.fragment().is_some() {
            return Err("RUNGU_MCP_RESOURCE_URL must not have a query or fragment".into());
        }
        if resource.path() != "/mcp" {
            return Err(format!("RUNGU_MCP_RESOURCE_URL must end in /mcp (got path '{}')", resource.path()));
        }
        Ok(Self {
            issuer,
            resource,
            http,
            admin_emails,
            allow_unverified_email: false,
            cache: Mutex::default(),
            refresh: Mutex::default(),
        })
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The exact MCP URL; also the token audience.
    pub fn resource(&self) -> &str {
        self.resource.as_str()
    }

    /// Where clients find the protected-resource metadata for `/mcp`.
    pub fn metadata_url(&self) -> String {
        format!("{}/.well-known/oauth-protected-resource/mcp", self.resource.origin().ascii_serialization())
    }

    /// The `WWW-Authenticate` value that starts a client's OAuth flow.
    /// `invalid_token` (RFC 6750) tells a client its token was refused, as
    /// opposed to missing.
    pub fn challenge(&self, invalid_token: bool) -> String {
        let error = if invalid_token { "error=\"invalid_token\", " } else { "" };
        format!("Bearer {error}resource_metadata=\"{}\"", self.metadata_url())
    }

    /// RFC 9728 protected-resource metadata.
    pub fn protected_resource_metadata(&self) -> Value {
        serde_json::json!({
            "resource": self.resource(),
            "authorization_servers": [self.issuer],
            "bearer_methods_supported": ["header"],
            "resource_name": "Rungu",
        })
    }

    /// The issuer's own metadata document, passed through unchanged. Clients
    /// that look for it on the resource origin find the real endpoints.
    pub async fn issuer_metadata(&self) -> Result<Value, Reject> {
        Ok(self.discovery().await?.raw)
    }

    async fn get_json(&self, url: &str, bearer: Option<&str>) -> Result<Value, Reject> {
        let mut request = self.http.get(url).timeout(FETCH_TIMEOUT).header("accept", "application/json");
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        let fail = |what: &str, detail: String| {
            tracing::warn!("MCP OAuth: {what} failed: {detail}");
            Reject::IssuerUnavailable
        };
        let response = request.send().await.map_err(|e| fail(url, e.to_string()))?;
        if !response.status().is_success() {
            return Err(fail(url, format!("HTTP {}", response.status())));
        }
        let mut response = response;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| fail(url, e.to_string()))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_DOC_BYTES {
                return Err(fail(url, "response too large".into()));
            }
        }
        serde_json::from_slice(&bytes).map_err(|e| fail(url, format!("not JSON: {e}")))
    }

    async fn discovery(&self) -> Result<Discovery, Reject> {
        if let Some(d) = self.cached_discovery().await? {
            return Ok(d);
        }
        let _one_at_a_time = self.refresh.lock().await;
        // Someone may have refreshed while this request waited.
        if let Some(d) = self.cached_discovery().await? {
            return Ok(d);
        }
        let result = self.fetch_discovery().await;
        let mut cache = self.cache.lock().await;
        match &result {
            Ok(d) => {
                cache.discovery = Some((Instant::now(), d.clone()));
                cache.discovery_failed = None;
            }
            Err(_) => cache.discovery_failed = Some(Instant::now()),
        }
        result
    }

    /// [`Self::discovery`] for callers that already hold the refresh lock.
    async fn discovery_unlocked(&self) -> Result<Discovery, Reject> {
        if let Some(d) = self.cached_discovery().await? {
            return Ok(d);
        }
        let result = self.fetch_discovery().await;
        let mut cache = self.cache.lock().await;
        match &result {
            Ok(d) => {
                cache.discovery = Some((Instant::now(), d.clone()));
                cache.discovery_failed = None;
            }
            Err(_) => cache.discovery_failed = Some(Instant::now()),
        }
        result
    }

    /// A fresh cached document, or `Err` while a recent failure is backing off.
    async fn cached_discovery(&self) -> Result<Option<Discovery>, Reject> {
        let cache = self.cache.lock().await;
        if let Some((at, d)) = &cache.discovery
            && at.elapsed() < DISCOVERY_TTL
        {
            return Ok(Some(d.clone()));
        }
        if cache.discovery_failed.is_some_and(|t| t.elapsed() < DISCOVERY_BACKOFF) {
            return Err(Reject::IssuerUnavailable);
        }
        Ok(None)
    }

    async fn fetch_discovery(&self) -> Result<Discovery, Reject> {
        let raw = self.get_json(&format!("{}/.well-known/openid-configuration", self.issuer), None).await?;
        // OIDC Discovery: the document must name the issuer it was fetched for.
        if raw.get("issuer").and_then(Value::as_str).map(|i| i.trim_end_matches('/')) != Some(self.issuer.as_str()) {
            tracing::warn!("MCP OAuth: issuer metadata names a different issuer");
            return Err(Reject::IssuerUnavailable);
        }
        // Keys and the userinfo call (which carries the bearer token) must be https.
        let endpoint = |field: &str| {
            raw.get(field)
                .and_then(Value::as_str)
                .filter(|u| Url::parse(u).is_ok_and(|u| require_https(&u, field).is_ok()))
        };
        let jwks_uri = endpoint("jwks_uri").ok_or(Reject::IssuerUnavailable)?.to_string();
        let userinfo_endpoint = endpoint("userinfo_endpoint").map(str::to_string);
        Ok(Discovery { jwks_uri, userinfo_endpoint, raw })
    }

    /// The decoding key for a token's `kid`, refetching the JWKS when the key
    /// is unknown (rotation) — but never more than once per [`JWKS_MIN_REFETCH`].
    async fn key_for(&self, kid: Option<&str>) -> Result<DecodingKey, Reject> {
        if let Some(key) = self.lookup_key(kid).await? {
            return Ok(key);
        }
        let _one_at_a_time = self.refresh.lock().await;
        // Another request may have refreshed the keys while this one waited.
        if let Some(key) = self.lookup_key(kid).await? {
            return Ok(key);
        }
        self.cache.lock().await.jwks_fetched = Some(Instant::now());
        let uri = self.discovery_unlocked().await?.jwks_uri;
        let doc = self.get_json(&uri, None).await?;
        let set = parse_jwks(&doc).ok_or(Reject::IssuerUnavailable)?;
        self.cache.lock().await.jwks = Some((Instant::now(), set));
        self.lookup_key(kid).await?.ok_or(Reject::UnknownKey)
    }

    /// The key for `kid` from a fresh cached JWKS. `Err(UnknownKey)` when the
    /// cache is fresh or was refetched too recently to try again; `Ok(None)`
    /// when a refetch is allowed.
    async fn lookup_key(&self, kid: Option<&str>) -> Result<Option<DecodingKey>, Reject> {
        let cache = self.cache.lock().await;
        if let Some((at, set)) = &cache.jwks
            && at.elapsed() < JWKS_TTL
        {
            let jwk = match kid {
                Some(kid) => set.find(kid),
                None if set.keys.len() == 1 => set.keys.first(),
                None => None,
            };
            if let Some(jwk) = jwk {
                return DecodingKey::from_jwk(jwk).map(Some).map_err(|_| Reject::UnknownKey);
            }
        }
        if cache.jwks_fetched.is_some_and(|t| t.elapsed() < JWKS_MIN_REFETCH) {
            return Err(Reject::UnknownKey);
        }
        Ok(None)
    }

    /// An email must be marked verified. Only `false` used to fail, so an
    /// issuer that lets people type any address and omits the claim would
    /// have let them claim an admin's. Issuers that verify at signup but
    /// don't emit the claim opt in with `RUNGU_MCP_OAUTH_ALLOW_UNVERIFIED_EMAIL`.
    fn check_verified(&self, verified: Option<bool>) -> Result<(), Reject> {
        match verified {
            Some(true) => Ok(()),
            None if self.allow_unverified_email => Ok(()),
            _ => Err(Reject::UnverifiedEmail),
        }
    }

    /// Verify an access token and resolve the Rungu user behind it.
    pub async fn authenticate(&self, token: &str, store: &Store) -> Result<Actor, Reject> {
        if token.split('.').count() != 3 {
            return Err(Reject::Opaque);
        }
        let header = jsonwebtoken::decode_header(token).map_err(|_| Reject::Invalid("malformed_token"))?;
        if !ALGORITHMS.contains(&header.alg) {
            return Err(Reject::UnsupportedAlgorithm);
        }
        let key = self.key_for(header.kid.as_deref()).await?;

        let mut validation = Validation::new(header.alg);
        // Some issuers (Auth0) put a trailing slash in `iss`; accept either form.
        validation.set_issuer(&[self.issuer.as_str(), &format!("{}/", self.issuer)]);
        validation.set_audience(&[self.resource()]);
        validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = 30;
        let claims = jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .map_err(|e| {
                use jsonwebtoken::errors::ErrorKind::*;
                Reject::Invalid(match e.kind() {
                    ExpiredSignature => "expired",
                    ImmatureSignature => "not_yet_valid",
                    InvalidIssuer => "wrong_issuer",
                    InvalidAudience => "wrong_audience",
                    InvalidSignature => "bad_signature",
                    MissingRequiredClaim(_) => "missing_claim",
                    _ => "invalid_token",
                })
            })?
            .claims;

        if claims.cnf.is_some() {
            return Err(Reject::SenderConstrained);
        }
        let sub = claims.sub.filter(|s| !s.is_empty()).ok_or(Reject::NoSubject)?;

        // A claim the token itself makes about its email is judged on every
        // request, before any cached identity is considered.
        if claims.email.is_some() {
            self.check_verified(claims.email_verified)?;
        }
        let claimed_email = claims.email.as_deref().map(|e| e.trim().to_lowercase());
        if let Some((at, actor, email)) = self.cache.lock().await.actors.get(&sub)
            && at.elapsed() < ACTOR_TTL
            && claimed_email.as_ref().is_none_or(|e| e == email)
        {
            return Ok(actor.clone());
        }

        // Gatehouse access tokens carry no email: ask the issuer who this is.
        let (email, name) = match claims.email {
            Some(email) => (email, claims.name),
            None => {
                let endpoint = self.discovery().await?.userinfo_endpoint.ok_or(Reject::NoEmail)?;
                let info = self.get_json(&endpoint, Some(token)).await?;
                // OIDC: userinfo must name the same subject as the token.
                if info.get("sub").and_then(Value::as_str) != Some(sub.as_str()) {
                    return Err(Reject::Invalid("userinfo_subject_mismatch"));
                }
                let email = info.get("email").and_then(Value::as_str).ok_or(Reject::NoEmail)?.to_string();
                self.check_verified(info.get("email_verified").and_then(Value::as_bool))?;
                (email, info.get("name").and_then(Value::as_str).map(str::to_string))
            }
        };
        let email = email.trim().to_lowercase();
        if email.is_empty() {
            return Err(Reject::NoEmail);
        }
        // Built-in identities must never be reachable by naming their address.
        if email == rungu_auth::middleware::AGENT_USER_EMAIL || email == "mcp@rungu.local" {
            return Err(Reject::Invalid("reserved_email"));
        }

        let user = store.find_or_create_user(&email, name.as_deref(), None, &self.admin_emails).await.map_err(|e| {
            tracing::error!("MCP OAuth: resolving user failed: {e}");
            Reject::IssuerUnavailable
        })?;
        let actor = Actor { id: user.id, role: user.role };

        let mut cache = self.cache.lock().await;
        if cache.actors.len() >= MAX_CACHED_ACTORS {
            cache.actors.retain(|_, (at, _, _)| at.elapsed() < ACTOR_TTL);
            if cache.actors.len() >= MAX_CACHED_ACTORS {
                cache.actors.clear();
            }
        }
        cache.actors.insert(sub, (Instant::now(), actor.clone(), email));
        Ok(actor)
    }
}

/// The usable signing keys of a JWKS document. One key this library can't
/// parse (an unusual type, say) must not take verification down for the rest,
/// and keys not meant for signatures are skipped.
fn parse_jwks(doc: &Value) -> Option<JwkSet> {
    let keys = doc.get("keys")?.as_array()?;
    let keys: Vec<_> = keys
        .iter()
        .filter(|k| k.get("use").and_then(Value::as_str).is_none_or(|u| u == "sig"))
        .filter_map(|k| serde_json::from_value(k.clone()).ok())
        .collect();
    Some(JwkSet { keys })
}

/// HTTPS is required, except for loopback (local development and tests).
pub(crate) fn require_https(url: &Url, name: &str) -> Result<(), String> {
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err(format!("{name} must be an https:// URL"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn one_unusable_key_does_not_break_the_rest() {
        let doc = json!({ "keys": [
            { "kty": "weird", "kid": "bad" },
            { "kty": "EC", "crv": "P-256", "kid": "enc-key", "use": "enc", "x": "AA", "y": "AA" },
            { "kty": "EC", "crv": "P-256", "kid": "good", "use": "sig", "x": "AA", "y": "AA" },
        ]});
        let set = parse_jwks(&doc).unwrap();
        let kids: Vec<_> = set.keys.iter().filter_map(|k| k.common.key_id.as_deref()).collect();
        assert_eq!(kids, ["good"]);
        assert!(parse_jwks(&json!({ "nope": 1 })).is_none());
    }
}
