//! Generic OIDC login (#267) against a fake issuer: discovery, PKCE, the
//! code exchange, userinfo, and the identity rules.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use rungu_api::oidc_login::{OidcLogin, s256_challenge};
use rungu_api::{AppState, auth_routes};
use rungu_auth::AuthConfig;
use rungu_core::{Store, open_pool, run_migrations};
use serde_json::{Value, json};
use tower::ServiceExt;

const APP_URL: &str = "http://localhost:3000";

/// What the fake issuer remembers between the browser steps and the server's calls.
#[derive(Clone, Default)]
struct IssuerState {
    /// The `code_challenge` the test saw on the login redirect.
    challenge: Arc<Mutex<Option<String>>>,
    userinfo: Arc<Mutex<Value>>,
    token_calls: Arc<AtomicUsize>,
    /// The last form the token endpoint received.
    last_token_form: Arc<Mutex<Vec<(String, String)>>>,
    /// How many times discovery was asked, and whether it currently fails.
    discovery_hits: Arc<AtomicUsize>,
    discovery_down: Arc<AtomicBool>,
}

struct Issuer {
    url: String,
    state: IssuerState,
}

fn good_userinfo() -> Value {
    json!({ "sub": "gh-user-1", "email": "Alice@Example.test", "email_verified": true, "name": "Alice" })
}

async fn start_issuer() -> Issuer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let state = IssuerState::default();
    *state.userinfo.lock().unwrap() = good_userinfo();

    let doc = json!({
        "issuer": url,
        "authorization_endpoint": format!("{url}/authorize"),
        "token_endpoint": format!("{url}/token"),
        "userinfo_endpoint": format!("{url}/userinfo"),
        "code_challenge_methods_supported": ["S256"],
    });
    let app = axum::Router::new()
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move |State(s): State<IssuerState>| {
                let doc = doc.clone();
                async move {
                    s.discovery_hits.fetch_add(1, Ordering::SeqCst);
                    if s.discovery_down.load(Ordering::SeqCst) {
                        return (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({})));
                    }
                    (StatusCode::OK, axum::Json(doc))
                }
            }),
        )
        .route(
            "/token",
            axum::routing::post(
                |State(s): State<IssuerState>,
                 axum::extract::Form(form): axum::extract::Form<Vec<(String, String)>>| async move {
                    s.token_calls.fetch_add(1, Ordering::SeqCst);
                    *s.last_token_form.lock().unwrap() = form.clone();
                    let field = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
                    // The issuer redeems the code only for the holder of the PKCE secret.
                    let challenge = s.challenge.lock().unwrap().clone().unwrap_or_default();
                    let pkce_ok = !challenge.is_empty() && s256_challenge(&field("code_verifier")) == challenge;
                    if field("code") != "good-code" || field("client_id") != "rungu-client" || !pkce_ok {
                        return (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": "invalid_grant" })));
                    }
                    (StatusCode::OK, axum::Json(json!({ "access_token": "at-1", "token_type": "Bearer" })))
                },
            ),
        )
        .route(
            "/userinfo",
            axum::routing::get(|State(s): State<IssuerState>, headers: axum::http::HeaderMap| async move {
                if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer at-1") {
                    return (StatusCode::UNAUTHORIZED, axum::Json(json!({})));
                }
                (StatusCode::OK, axum::Json(s.userinfo.lock().unwrap().clone()))
            }),
        )
        .with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Issuer { url, state }
}

async fn app_for(issuer_url: Option<&str>) -> (axum::Router, Store) {
    let oidc = issuer_url.map(|url| {
        Arc::new(
            OidcLogin::new(url, "rungu-client", "rungu-secret", Some("CodeCora ID"), APP_URL, reqwest::Client::new())
                .unwrap(),
        )
    });
    app_with(oidc).await
}

async fn app_with(oidc: Option<Arc<OidcLogin>>) -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: APP_URL.to_string(),
        secure_cookie: false,
        admin_emails: vec!["boss@example.test".to_string()],
        api_key: None,
        google: None,
        github: None,
        keycloak: None,
    };
    let state = AppState::new(
        store.clone(),
        config,
        reqwest::Client::new(),
        Arc::from(rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-oidc")).unwrap()),
        rungu_api::meta::InstanceBranding::default(),
        Arc::new(rungu_api::meta::LicenseStatus::new()),
        Arc::new(None),
    )
    .with_oidc(oidc);
    (axum::Router::new().merge(auth_routes().with_state(state)), store)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

impl Reply {
    fn location(&self) -> Url {
        Url::parse(self.headers["location"].to_str().unwrap()).unwrap()
    }
    /// The value of the `Set-Cookie` named `name` (ignoring attributes), if one is set.
    fn cookie(&self, name: &str) -> Option<String> {
        self.headers.get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find_map(|c| {
            c.strip_prefix(&format!("{name}=")).map(|rest| rest.split(';').next().unwrap_or("").to_string())
        })
    }
    fn raw_cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with(&format!("{name}=")))
            .map(str::to_string)
    }
}

use url::Url;

async fn get(app: &axum::Router, uri: &str, cookies: &str) -> Reply {
    let mut req = Request::builder().uri(uri);
    if !cookies.is_empty() {
        req = req.header("cookie", cookies);
    }
    let response = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    Reply { status, headers, json: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
}

fn param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned())
}

/// Start a login and return what the browser would carry to the callback:
/// `(state, cookie header, the issuer-side challenge is recorded)`.
async fn start_login(app: &axum::Router, issuer: &Issuer) -> (String, String) {
    let r = get(app, "/auth/oidc/login", "").await;
    assert_eq!(r.status, StatusCode::FOUND);
    let loc = r.location();
    let state = param(&loc, "state").expect("state");
    *issuer.state.challenge.lock().unwrap() = param(&loc, "code_challenge");
    let cookies =
        format!("oauth_state={}; oauth_pkce={}", r.cookie("oauth_state").unwrap(), r.cookie("oauth_pkce").unwrap());
    (state, cookies)
}

#[tokio::test]
async fn the_provider_is_listed_with_its_label() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let r = get(&app, "/auth/providers", "").await;
    let providers = r.json["providers"].as_array().unwrap();
    let oidc = providers.iter().find(|p| p["name"] == "oidc").expect("oidc listed");
    assert_eq!(oidc["label"], "CodeCora ID");
    assert_eq!(oidc["login_url"], format!("{APP_URL}/auth/oidc/login"));

    // Not configured: not listed, and the routes are 404.
    let (off, _) = app_for(None).await;
    assert!(get(&off, "/auth/providers", "").await.json["providers"].as_array().unwrap().is_empty());
    assert_eq!(get(&off, "/auth/oidc/login", "").await.status, StatusCode::NOT_FOUND);
    // (The callback checks state first, so give it a matching one to reach the provider lookup.)
    assert_eq!(get(&off, "/auth/oidc/callback?code=x&state=y", "oauth_state=y").await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn login_redirects_to_the_issuer_with_pkce_and_state() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let r = get(&app, "/auth/oidc/login?redirect=/board/demo", "").await;
    assert_eq!(r.status, StatusCode::FOUND);

    let loc = r.location();
    assert_eq!(format!("{}://{}{}", loc.scheme(), loc.authority(), loc.path()), format!("{}/authorize", issuer.url));
    assert_eq!(param(&loc, "response_type").as_deref(), Some("code"));
    assert_eq!(param(&loc, "client_id").as_deref(), Some("rungu-client"));
    assert_eq!(param(&loc, "redirect_uri").as_deref(), Some(format!("{APP_URL}/auth/oidc/callback").as_str()));
    let scope = param(&loc, "scope").unwrap();
    for s in ["openid", "email", "profile"] {
        assert!(scope.split(' ').any(|x| x == s), "scope {scope}");
    }
    assert_eq!(param(&loc, "code_challenge_method").as_deref(), Some("S256"));

    // The challenge is the S256 of the verifier held in the HttpOnly cookie.
    let verifier = r.cookie("oauth_pkce").expect("pkce cookie");
    assert_eq!(param(&loc, "code_challenge").unwrap(), s256_challenge(&verifier));
    assert!(r.raw_cookie("oauth_pkce").unwrap().contains("HttpOnly"));
    assert_eq!(param(&loc, "state").unwrap(), r.cookie("oauth_state").unwrap());
    assert!(!loc.query().unwrap().contains(&verifier), "the verifier never leaves the cookie");

    // Every login gets a fresh pair.
    let again = get(&app, "/auth/oidc/login", "").await;
    assert_ne!(again.cookie("oauth_pkce"), r.cookie("oauth_pkce"));
}

#[tokio::test]
async fn a_full_login_signs_in_and_finds_the_same_person_next_time() {
    let issuer = start_issuer().await;
    let (app, store) = app_for(Some(&issuer.url)).await;

    let (state, cookies) = start_login(&app, &issuer).await;
    let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &cookies).await;
    assert_eq!(r.status, StatusCode::FOUND, "{:?}", r.json);
    assert_eq!(r.headers["location"], "/");
    assert!(r.cookie("session").is_some_and(|s| !s.is_empty()), "a session cookie is set");
    assert_eq!(r.cookie("oauth_pkce").as_deref(), Some(""), "the verifier cookie is cleared");
    assert_eq!(r.cookie("oauth_state").as_deref(), Some(""));

    // The code was redeemed with the verifier and the redirect URI.
    let form = issuer.state.last_token_form.lock().unwrap().clone();
    let field = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(field("grant_type").as_deref(), Some("authorization_code"));
    assert_eq!(field("redirect_uri").as_deref(), Some(format!("{APP_URL}/auth/oidc/callback").as_str()));
    assert!(field("code_verifier").is_some());

    // Found or created by (lower-cased) email, with the provider identity linked.
    let alice = store.find_or_create_user("alice@example.test", None, None, &[]).await.unwrap();
    assert_eq!(alice.role, rungu_proto::UserRole::Member);

    // A second login is the same person.
    let (state, cookies) = start_login(&app, &issuer).await;
    let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &cookies).await;
    assert_eq!(r.status, StatusCode::FOUND);
    let again = store.find_or_create_user("alice@example.test", None, None, &[]).await.unwrap();
    assert_eq!(again.id, alice.id);
}

#[tokio::test]
async fn admin_emails_still_decide_the_role() {
    let issuer = start_issuer().await;
    let (app, store) = app_for(Some(&issuer.url)).await;
    *issuer.state.userinfo.lock().unwrap() =
        json!({ "sub": "boss-1", "email": "boss@example.test", "email_verified": true });
    let (state, cookies) = start_login(&app, &issuer).await;
    let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &cookies).await;
    assert_eq!(r.status, StatusCode::FOUND);
    let boss = store.find_or_create_user("boss@example.test", None, None, &[]).await.unwrap();
    assert_eq!(boss.role, rungu_proto::UserRole::Admin);
}

#[tokio::test]
async fn an_unverified_or_missing_email_claim_refuses_the_login() {
    for userinfo in [
        json!({ "sub": "u", "email": "x@example.test", "email_verified": false }),
        json!({ "sub": "u", "email": "x@example.test" }),
        json!({ "sub": "u", "email": "x@example.test", "email_verified": "true" }),
    ] {
        let issuer = start_issuer().await;
        let (app, store) = app_for(Some(&issuer.url)).await;
        *issuer.state.userinfo.lock().unwrap() = userinfo.clone();
        let (state, cookies) = start_login(&app, &issuer).await;
        let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &cookies).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{userinfo}");
        assert!(r.cookie("session").is_none(), "no session for an unverified email");
        // And no account was created for it.
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = 'x@example.test'")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(accounts, 0, "{userinfo}");
    }
}

#[tokio::test]
async fn the_callback_checks_state_and_the_pkce_secret() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let (state, cookies) = start_login(&app, &issuer).await;

    // Wrong state (CSRF) and no code or state at all.
    let r = get(&app, "/auth/oidc/callback?code=good-code&state=forged", &cookies).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/auth/oidc/callback", &cookies).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(issuer.state.token_calls.load(Ordering::SeqCst), 0, "the issuer is never asked");

    // No PKCE cookie: refused before the issuer is called.
    let only_state = cookies.split("; ").next().unwrap().to_string();
    let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &only_state).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(issuer.state.token_calls.load(Ordering::SeqCst), 0);

    // A different verifier does not redeem the code: the issuer refuses.
    let tampered = format!("{only_state}; oauth_pkce=not-the-verifier");
    let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &tampered).await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    assert!(r.cookie("session").is_none());
    assert_eq!(issuer.state.token_calls.load(Ordering::SeqCst), 1);

    // A wrong code is refused too, and a provider error never reaches the issuer.
    let r = get(&app, &format!("/auth/oidc/callback?code=bad-code&state={state}"), &cookies).await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    let r = get(&app, "/auth/oidc/callback?error=access_denied", &cookies).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_unreachable_or_wrong_issuer_fails_the_login_not_the_server() {
    // Nothing listens there: 502, and the app still serves other routes.
    let (app, _) = app_for(Some("http://127.0.0.1:9")).await;
    assert_eq!(get(&app, "/auth/oidc/login", "").await.status, StatusCode::BAD_GATEWAY);
    assert_eq!(get(&app, "/auth/providers", "").await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_discovery_document_naming_another_issuer_is_refused() {
    // `localhost` reaches the same fake issuer, but its document names the 127.0.0.1 origin.
    let issuer = start_issuer().await;
    let other_name = issuer.url.replace("127.0.0.1", "localhost");
    let login = OidcLogin::new(&other_name, "rungu-client", "s", None, APP_URL, reqwest::Client::new()).unwrap();
    let err = login.provider().await.unwrap_err();
    assert!(err.contains("different issuer"), "{err}");
}

#[tokio::test]
async fn built_in_identities_cannot_log_in_through_a_provider() {
    for email in ["agent@rungu.local", "MCP@rungu.local", "  agent@rungu.local "] {
        let issuer = start_issuer().await;
        let (app, store) = app_for(Some(&issuer.url)).await;
        *issuer.state.userinfo.lock().unwrap() = json!({ "sub": "x", "email": email, "email_verified": true });
        let (state, cookies) = start_login(&app, &issuer).await;
        let r = get(&app, &format!("/auth/oidc/callback?code=good-code&state={state}"), &cookies).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{email:?}");
        assert!(r.cookie("session").is_none(), "{email:?}: no session for a built-in identity");
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email LIKE '%@rungu.local'")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(accounts, 0, "{email:?}: nothing was created or reached");
    }
}

#[tokio::test]
async fn a_redirect_with_a_control_character_is_dropped() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    // A tab is stripped by browsers' URL parsers: `/<TAB>/evil.example` would become `//evil.example`.
    for bad in ["/%09/evil.example", "/%0A/x", "/%0D%0A", "/%7F/x", "//evil.example", "/%5Cevil.example"] {
        let r = get(&app, &format!("/auth/oidc/login?redirect={bad}"), "").await;
        assert_eq!(r.status, StatusCode::FOUND, "{bad}");
        assert!(r.cookie("oauth_redirect").is_none(), "{bad}: no redirect cookie is signed for it");
    }
    let ok = get(&app, "/auth/oidc/login?redirect=/board/demo", "").await;
    assert!(ok.cookie("oauth_redirect").is_some_and(|c| !c.is_empty()));
}

#[tokio::test]
async fn a_burst_of_logins_asks_the_issuer_once() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let logins: Vec<_> = (0..12)
        .map(|_| {
            let app = app.clone();
            tokio::spawn(async move { get(&app, "/auth/oidc/login", "").await.status })
        })
        .collect();
    for login in logins {
        assert_eq!(login.await.unwrap(), StatusCode::FOUND);
    }
    assert_eq!(issuer.state.discovery_hits.load(Ordering::SeqCst), 1, "one discovery for the whole burst");
    // And the cached document serves later logins.
    get(&app, "/auth/oidc/login", "").await;
    assert_eq!(issuer.state.discovery_hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_down_issuer_is_not_hammered_and_the_last_good_document_keeps_serving() {
    // Expired immediately, so every login needs a refresh.
    let issuer = start_issuer().await;
    let login = OidcLogin::new(&issuer.url, "rungu-client", "s", None, APP_URL, reqwest::Client::new())
        .unwrap()
        .with_cache_ttl(std::time::Duration::ZERO);
    let (app, _) = app_with(Some(Arc::new(login))).await;
    assert_eq!(get(&app, "/auth/oidc/login", "").await.status, StatusCode::FOUND);
    assert_eq!(issuer.state.discovery_hits.load(Ordering::SeqCst), 1);

    // The issuer goes down: the refresh fails once, then logins keep working from the
    // last good document and the issuer is left alone for a while.
    issuer.state.discovery_down.store(true, Ordering::SeqCst);
    for _ in 0..6 {
        assert_eq!(get(&app, "/auth/oidc/login", "").await.status, StatusCode::FOUND);
    }
    assert_eq!(issuer.state.discovery_hits.load(Ordering::SeqCst), 2, "one failed refresh, then back-off");

    // With no good document at all, a down issuer is a 502, also without hammering.
    let down = start_issuer().await;
    down.state.discovery_down.store(true, Ordering::SeqCst);
    let (cold, _) = app_for(Some(&down.url)).await;
    for _ in 0..4 {
        assert_eq!(get(&cold, "/auth/oidc/login", "").await.status, StatusCode::BAD_GATEWAY);
    }
    assert_eq!(down.state.discovery_hits.load(Ordering::SeqCst), 1);
}
