#![allow(dead_code)]
//! Shared by the OAuth tests (`/mcp` #253, REST #272): a fake issuer and app setup.
//!
//! The issuer is a local axum server serving OIDC metadata, a JWKS and
//! userinfo; tokens are signed with an ES256 key generated per run, so no key
//! material lives in the repo.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use rungu_api::mcp_oauth::{McpOAuth, McpSettings};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_core::{Store, open_pool, run_migrations};
use serde_json::{Value, json};
use tower::ServiceExt;

pub const API_KEY: &str = "test-api-key-0123456789abcdef";
pub const RESOURCE: &str = "http://localhost:3000/mcp";
pub const API_RESOURCE: &str = "http://localhost:3000/api";
pub const ADMIN: &str = "boss@example.test";

/// A signing key and the JWKS entry that verifies it.
pub struct TestKey {
    pub der: Vec<u8>,
    pub jwk: Value,
}

pub fn test_key(kid: &str) -> TestKey {
    let rng = SystemRandom::new();
    let der = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap().as_ref().to_vec();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &rng).unwrap();
    let point = pair.public_key().as_ref(); // 0x04 || X || Y
    let jwk = json!({
        "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": kid,
        "x": B64.encode(&point[1..33]), "y": B64.encode(&point[33..65]),
    });
    TestKey { der, jwk }
}

pub struct Issuer {
    pub url: String,
    pub key: TestKey,
    pub jwks_hits: Arc<AtomicUsize>,
    /// While set, userinfo answers 401 (a banned person / revoked token).
    pub revoked: Arc<AtomicBool>,
}

/// Start the fake issuer. Its userinfo knows one person per token `sub`.
pub async fn start_issuer() -> Issuer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let key = test_key("k1");
    let jwks = json!({ "keys": [key.jwk.clone()] });
    let jwks_hits = Arc::new(AtomicUsize::new(0));
    let hits = jwks_hits.clone();
    let revoked = Arc::new(AtomicBool::new(false));
    let userinfo_revoked = revoked.clone();
    let metadata = json!({
        "issuer": url,
        "jwks_uri": format!("{url}/jwks"),
        "userinfo_endpoint": format!("{url}/userinfo"),
        "authorization_endpoint": format!("{url}/authorize"),
        "token_endpoint": format!("{url}/token"),
    });
    let app = axum::Router::new()
        .route("/.well-known/openid-configuration", axum::routing::get(move || async move { axum::Json(metadata) }))
        .route(
            "/jwks",
            axum::routing::get(move || async move {
                hits.fetch_add(1, Ordering::SeqCst);
                axum::Json(jwks)
            }),
        )
        .route(
            "/userinfo",
            axum::routing::get(move |headers: axum::http::HeaderMap| async move {
                if headers.get("authorization").is_none() || userinfo_revoked.load(Ordering::SeqCst) {
                    return (StatusCode::UNAUTHORIZED, axum::Json(json!({}))).into_response_pair();
                }
                (StatusCode::OK, axum::Json(json!({ "sub": "user-1", "email": "Alice@Example.test", "email_verified": true, "name": "Alice" })))
                    .into_response_pair()
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Issuer { url, key, jwks_hits, revoked }
}

pub trait IntoPair {
    fn into_response_pair(self) -> (StatusCode, axum::Json<Value>);
}
impl IntoPair for (StatusCode, axum::Json<Value>) {
    fn into_response_pair(self) -> (StatusCode, axum::Json<Value>) {
        self
    }
}

pub fn token_with(issuer: &Issuer, key_der: &[u8], kid: &str, claims: Value) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(kid.to_string());
    jsonwebtoken::encode(&header, &claims, &EncodingKey::from_ec_der(key_der)).unwrap_or_else(|e| {
        panic!("signing failed for {}: {e}", issuer.url);
    })
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn good_claims(issuer: &Issuer) -> Value {
    json!({ "iss": issuer.url, "aud": [RESOURCE, format!("{}/userinfo", issuer.url)], "sub": "user-1",
            "exp": now() + 3600, "iat": now() })
}

pub fn good_token(issuer: &Issuer) -> String {
    token_with(issuer, &issuer.key.der, "k1", good_claims(issuer))
}

/// Settings the OAuth tests vary.
#[derive(Default, Clone, Copy)]
pub struct Opts<'a> {
    /// `RUNGU_API_RESOURCE_URL`: REST accepts issuer tokens for this audience.
    pub api_resource: Option<&'a str>,
    /// `RUNGU_MCP_OAUTH_ALLOW_UNVERIFIED_EMAIL` (applies to `/mcp` only).
    pub allow_unverified_email: bool,
}

pub async fn app_for(issuer_url: Option<&str>) -> (axum::Router, Store) {
    app_with(issuer_url, Opts::default()).await
}

pub async fn app_with(issuer_url: Option<&str>, opts: Opts<'_>) -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Test App", "test-app", "").await.unwrap();
    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: "http://localhost:3000".to_string(),
        secure_cookie: false,
        admin_emails: vec![ADMIN.to_string()],
        api_key: Some(API_KEY.to_string()),
        google: None,
        github: None,
        keycloak: None,
    };
    let mcp = McpSettings {
        oauth: issuer_url.map(|url| {
            let mut oauth = McpOAuth::new(url, RESOURCE, config.admin_emails.clone(), reqwest::Client::new()).unwrap();
            if opts.allow_unverified_email {
                oauth = oauth.allowing_unverified_email();
            }
            if let Some(api) = opts.api_resource {
                oauth = oauth.with_api_resource(api).unwrap();
            }
            Arc::new(oauth)
        }),
        extra_origins: vec!["https://inspector.example".to_string()],
    };
    let state = AppState::new(
        store.clone(),
        config,
        reqwest::Client::new(),
        Arc::from(rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-oauth")).unwrap()),
        rungu_api::meta::InstanceBranding::default(),
        Arc::new(rungu_api::meta::LicenseStatus::new()),
        Arc::new(None),
    )
    .with_mcp(mcp);
    let app = axum::Router::new()
        .merge(api_routes().with_state(state.clone()))
        .merge(rungu_api::mcp_http::mcp_routes().with_state(state));
    (app, store)
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub json: Value,
}

pub async fn send(app: &axum::Router, method: &str, uri: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let response = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    Reply { status, headers, json: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
}
