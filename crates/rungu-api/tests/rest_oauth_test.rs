//! Issuer-signed Bearer tokens on the REST API (#272), against the fake issuer
//! shared with the `/mcp` tests (see `common`).

mod common;

use std::sync::atomic::Ordering;

use axum::http::StatusCode;
use common::*;
use rungu_api::mcp_oauth::McpOAuth;
use rungu_core::Store;
use rungu_proto::{CurrentUser, UserRole};
use serde_json::{Value, json};

/// Claims for a REST token: the REST audience, with the email in the token.
fn api_claims(issuer: &Issuer, sub: &str, email: &str) -> Value {
    let mut claims = good_claims(issuer);
    claims["aud"] = json!([API_RESOURCE]);
    claims["sub"] = json!(sub);
    claims["email"] = json!(email);
    claims["email_verified"] = json!(true);
    claims
}

fn sign(issuer: &Issuer, claims: Value) -> String {
    token_with(issuer, &issuer.key.der, "k1", claims)
}

fn api_token(issuer: &Issuer, sub: &str, email: &str) -> String {
    sign(issuer, api_claims(issuer, sub, email))
}

fn api_opts() -> Opts<'static> {
    Opts { api_resource: Some(API_RESOURCE), ..Opts::default() }
}

async fn rest(app: &axum::Router, method: &str, uri: &str, headers: &[(&str, &str)], body: Value) -> Reply {
    let mut all = vec![("content-type", "application/json")];
    all.extend_from_slice(headers);
    send(app, method, uri, &all, &body.to_string()).await
}

/// `POST /projects/test-app/posts` with a Bearer.
async fn create_post(app: &axum::Router, bearer: &str) -> Reply {
    let auth = format!("Bearer {bearer}");
    rest(app, "POST", "/projects/test-app/posts", &[("authorization", &auth)], json!({ "title": "From a product" }))
        .await
}

async fn mcp_list(app: &axum::Router, bearer: &str) -> Reply {
    let auth = format!("Bearer {bearer}");
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string();
    send(app, "POST", "/mcp", &[("authorization", &auth), ("content-type", "application/json")], &body).await
}

async fn user(store: &Store, email: &str) -> rungu_proto::User {
    store.find_or_create_user(email, None, None, &[]).await.unwrap()
}

#[tokio::test]
async fn a_valid_token_acts_as_the_person_and_authors_what_they_write() {
    let issuer = start_issuer().await;
    let (app, store) = app_with(Some(&issuer.url), api_opts()).await;

    let r = create_post(&app, &api_token(&issuer, "alice-sub", "Alice@Example.test")).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let alice = user(&store, "alice@example.test").await;
    assert_eq!(r.json["data"]["created_by"], alice.id);
    assert_eq!(alice.role, UserRole::Member);

    // No email in the token: resolved through userinfo, like for /mcp.
    let r = create_post(&app, &good_token_for(&issuer, API_RESOURCE)).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
}

fn good_token_for(issuer: &Issuer, audience: &str) -> String {
    let mut claims = good_claims(issuer);
    claims["aud"] = json!([audience, format!("{}/userinfo", issuer.url)]);
    sign(issuer, claims)
}

#[tokio::test]
async fn a_token_never_carries_admin_rights_and_the_stored_role_is_untouched() {
    let issuer = start_issuer().await;
    let (app, store) = app_with(Some(&issuer.url), api_opts()).await;
    let boss_token = api_token(&issuer, "boss-sub", ADMIN);
    let auth = format!("Bearer {boss_token}");
    let h = [("authorization", auth.as_str())];

    // An admin-only route.
    let r = rest(&app, "POST", "/projects", &h, json!({ "name": "Nope" })).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.json);

    // An admin-only effect: another person's post.
    let alice = user(&store, "alice@example.test").await;
    let post = store
        .create_post(
            &store.get_project_by_slug("test-app").await.unwrap().unwrap().id,
            "Alice's idea",
            "",
            rungu_proto::PostCategory::Feature,
            &alice.id,
        )
        .await
        .unwrap();
    let uri = format!("/posts/{}", post.id);
    let r = rest(&app, "PATCH", &uri, &h, json!({ "status": "planned" })).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.json);

    // The stored role is what ADMIN_EMAILS made it; only this request is capped.
    let stored = store.find_or_create_user(ADMIN, None, None, &[]).await.unwrap();
    assert_eq!(stored.role, UserRole::Admin);

    // The API key (and, by extension, the web session) still administers.
    let key = format!("Bearer {API_KEY}");
    let r = rest(&app, "PATCH", &uri, &[("authorization", &key)], json!({ "status": "planned" })).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
}

#[tokio::test]
async fn audiences_are_separate_for_rest_and_mcp() {
    let issuer = start_issuer().await;
    let (app, _) = app_with(Some(&issuer.url), api_opts()).await;

    // A token minted for /mcp is not valid for REST...
    let for_mcp = good_token_for(&issuer, RESOURCE);
    let r = create_post(&app, &for_mcp).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let challenge = r.headers["www-authenticate"].to_str().unwrap();
    assert!(challenge.starts_with("Bearer") && challenge.contains("error=\"invalid_token\""), "{challenge}");
    assert!(!r.json.to_string().contains(&for_mcp), "the token is never echoed");

    // ...and the other way round.
    let for_rest = good_token_for(&issuer, API_RESOURCE);
    assert_eq!(mcp_list(&app, &for_mcp).await.status, StatusCode::OK);
    assert_eq!(mcp_list(&app, &for_rest).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(create_post(&app, &for_rest).await.status, StatusCode::CREATED);

    // A token naming both is fine for both; any other audience for neither.
    let mut both = good_claims(&issuer);
    both["aud"] = json!([RESOURCE, API_RESOURCE, format!("{}/userinfo", issuer.url)]);
    let both = sign(&issuer, both);
    assert_eq!(create_post(&app, &both).await.status, StatusCode::CREATED);
    assert_eq!(mcp_list(&app, &both).await.status, StatusCode::OK);
    let other = good_token_for(&issuer, "http://localhost:3000/elsewhere");
    assert_eq!(create_post(&app, &other).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refused_tokens_get_401_with_a_challenge() {
    let issuer = start_issuer().await;
    let (app, _) = app_with(Some(&issuer.url), api_opts()).await;
    let forged = test_key("k1");
    let claims = |edit: &dyn Fn(&mut Value)| {
        let mut c = api_claims(&issuer, "s1", "x@example.test");
        edit(&mut c);
        sign(&issuer, c)
    };

    let cases: Vec<(&str, String)> = vec![
        ("expired", claims(&|c| c["exp"] = json!(now() - 3600))),
        ("not yet valid", claims(&|c| c["nbf"] = json!(now() + 3600))),
        ("wrong issuer", claims(&|c| c["iss"] = json!("http://evil.example"))),
        ("unverified email", claims(&|c| c["email_verified"] = json!(false))),
        (
            "email_verified missing",
            claims(&|c| {
                c.as_object_mut().unwrap().remove("email_verified");
            }),
        ),
        ("agent identity", claims(&|c| c["email"] = json!("agent@rungu.local"))),
        ("stdio identity", claims(&|c| c["email"] = json!("MCP@Rungu.Local"))),
        ("sender constrained", claims(&|c| c["cnf"] = json!({ "jkt": "abc" }))),
        ("forged signature", token_with(&issuer, &forged.der, "k1", api_claims(&issuer, "s1", "x@example.test"))),
        ("opaque", "opaque-access-token".to_string()),
        ("two segments", "aaa.bbb".to_string()),
    ];
    for (why, token) in cases {
        let r = create_post(&app, &token).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{why}");
        let challenge = r.headers["www-authenticate"].to_str().unwrap_or_default();
        assert!(challenge.contains("error=\"invalid_token\""), "{why}: {challenge}");
    }
}

#[tokio::test]
async fn a_token_the_issuer_refuses_at_userinfo_is_a_401_on_rest() {
    let issuer = start_issuer().await;
    let (app, _) = app_with(Some(&issuer.url), api_opts()).await;
    issuer.revoked.store(true, Ordering::SeqCst);
    let token = good_token_for(&issuer, API_RESOURCE);
    let r = create_post(&app, &token).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "a refused token is not an outage");
}

#[tokio::test]
async fn the_mcp_unverified_email_opt_out_is_not_inherited_by_rest() {
    let issuer = start_issuer().await;
    let opts = Opts { allow_unverified_email: true, ..api_opts() };
    let (app, _) = app_with(Some(&issuer.url), opts).await;

    let mut claims = api_claims(&issuer, "s-unmarked", "unmarked@example.test");
    claims.as_object_mut().unwrap().remove("email_verified");

    // /mcp, opted in: a token with no claim at all is accepted.
    let mut mcp_claims = claims.clone();
    mcp_claims["aud"] = json!([RESOURCE]);
    assert_eq!(mcp_list(&app, &sign(&issuer, mcp_claims)).await.status, StatusCode::OK);

    // REST: still refused, with the opt-out on.
    assert_eq!(create_post(&app, &sign(&issuer, claims)).await.status, StatusCode::UNAUTHORIZED);
    // An explicit `false` is refused everywhere; `true` works on REST.
    let mut explicit = api_claims(&issuer, "s-false", "false@example.test");
    explicit["email_verified"] = json!(false);
    assert_eq!(create_post(&app, &sign(&issuer, explicit)).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(create_post(&app, &api_token(&issuer, "s-true", "true@example.test")).await.status, StatusCode::CREATED);
}

#[tokio::test]
async fn an_identity_resolved_under_the_mcp_opt_out_does_not_serve_rest() {
    // The issuer never emits email_verified, and the operator opted /mcp in.
    // The access token carries no email, so the person is resolved through
    // userinfo and cached by subject. That cache is shared with REST, which
    // must still apply its own, stricter rule.
    let issuer = start_issuer().await;
    issuer.unmarked.store(true, Ordering::SeqCst);
    let opts = Opts { allow_unverified_email: true, ..api_opts() };
    let (app, _) = app_with(Some(&issuer.url), opts).await;

    // /mcp, opted in: accepted, and the identity is cached.
    assert_eq!(mcp_list(&app, &good_token_for(&issuer, RESOURCE)).await.status, StatusCode::OK);
    // REST, same person, inside the cache lifetime: refused all the same.
    let r = create_post(&app, &good_token_for(&issuer, API_RESOURCE)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "REST must not reuse an identity that /mcp accepted leniently");
    // The other way round is fine: a strictly resolved identity may serve /mcp.
    issuer.unmarked.store(false, Ordering::SeqCst);
    assert_eq!(create_post(&app, &good_token_for(&issuer, API_RESOURCE)).await.status, StatusCode::CREATED);
    assert_eq!(mcp_list(&app, &good_token_for(&issuer, RESOURCE)).await.status, StatusCode::OK);
}

#[tokio::test]
async fn an_unreachable_issuer_is_a_503() {
    let url = "http://127.0.0.1:9";
    let (app, _) = app_with(Some(url), api_opts()).await;
    let fake = Issuer {
        url: url.into(),
        key: test_key("k1"),
        jwks_hits: Default::default(),
        revoked: Default::default(),
        unmarked: Default::default(),
    };
    let r = create_post(&app, &api_token(&fake, "s1", "x@example.test")).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(r.headers.get("www-authenticate").is_none(), "no challenge: the token isn't at fault");
}

#[tokio::test]
async fn the_api_key_and_cookies_keep_working_and_a_wrong_key_never_falls_through() {
    let issuer = start_issuer().await;
    let (app, store) = app_with(Some(&issuer.url), api_opts()).await;

    // The API key, checked before anything else (an admin-only route).
    let key = format!("Bearer {API_KEY}");
    let r = rest(&app, "POST", "/projects", &[("authorization", &key)], json!({ "name": "By Key" })).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);

    // A session cookie.
    let bob = user(&store, "bob@example.test").await;
    let jwt = rungu_auth::session::issue_jwt(
        &CurrentUser { id: bob.id.clone(), email: bob.email.clone(), role: bob.role },
        "test-secret",
    )
    .unwrap();
    let cookie = format!("session={jwt}");
    let r = rest(&app, "POST", "/projects/test-app/posts", &[("cookie", &cookie)], json!({ "title": "Cookie" })).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["data"]["created_by"], bob.id);

    // A wrong Bearer next to a valid cookie is a 401: the Bearer decides.
    let wrong = "Bearer not-the-key";
    let r = rest(
        &app,
        "POST",
        "/projects/test-app/posts",
        &[("authorization", wrong), ("cookie", &cookie)],
        json!({ "title": "x" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let wrong_jwt = format!("Bearer {}", api_token(&test_issuer_forger(&issuer), "s", "x@example.test"));
    let r = rest(
        &app,
        "POST",
        "/projects/test-app/posts",
        &[("authorization", &wrong_jwt), ("cookie", &cookie)],
        json!({ "title": "x" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

/// An issuer handle that signs with a key the real one doesn't know.
fn test_issuer_forger(real: &Issuer) -> Issuer {
    Issuer {
        url: real.url.clone(),
        key: test_key("k1"),
        jwks_hits: Default::default(),
        revoked: Default::default(),
        unmarked: Default::default(),
    }
}

#[tokio::test]
async fn bearer_dispatch_has_no_fall_through() {
    let issuer = start_issuer().await;
    let (app, _) = app_with(Some(&issuer.url), api_opts()).await;

    // `rgk_` is reserved for integration keys: refused, and never tried as an
    // issuer token (even a perfectly valid JWT behind the prefix).
    let valid = api_token(&issuer, "s1", "x@example.test");
    for token in ["rgk_abc".to_string(), format!("rgk_{valid}"), "rgk_a.b.c".to_string()] {
        assert_eq!(create_post(&app, &token).await.status, StatusCode::UNAUTHORIZED, "{token}");
    }
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 0, "the issuer was never consulted");

    // A wrong value that is shaped like neither is refused too.
    assert_eq!(create_post(&app, "plain-secret").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 0);

    // The valid token does reach the issuer.
    assert_eq!(create_post(&app, &valid).await.status, StatusCode::CREATED);
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn without_the_rest_setting_a_non_key_bearer_is_a_plain_401() {
    let issuer = start_issuer().await;
    let token = api_token(&issuer, "s1", "x@example.test");
    // No issuer at all, or an issuer for /mcp only: nothing changes for REST.
    for url in [None, Some(issuer.url.as_str())] {
        let (app, _) = app_with(url, Opts::default()).await;
        for bearer in [token.as_str(), "opaque", "wrong-key"] {
            let r = create_post(&app, bearer).await;
            assert_eq!(r.status, StatusCode::UNAUTHORIZED);
            assert!(r.headers.get("www-authenticate").is_none(), "no challenge, as before");
            assert!(r.json.is_null(), "empty body, as before");
        }
        // The key still works.
        let key = format!("Bearer {API_KEY}");
        let r = rest(&app, "POST", "/projects", &[("authorization", &key)], json!({ "name": "K" })).await;
        assert_eq!(r.status, StatusCode::CREATED);
    }
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 0);
}

#[test]
fn the_rest_resource_is_validated_at_startup() {
    let base = || McpOAuth::new("https://issuer.example", "https://rungu.example/mcp", vec![], reqwest::Client::new());
    let with = |resource: &str| base().unwrap().with_api_resource(resource).err();
    assert!(with("not a url").is_some());
    assert!(with("http://rungu.example/api").unwrap().contains("https"));
    assert!(with("https://rungu.example/api?x=1").unwrap().contains("query"));
    assert!(with("https://rungu.example/api#frag").unwrap().contains("fragment"));
    assert!(with("https://rungu.example/mcp").unwrap().contains("differ"));
    assert!(with("https://rungu.example/api").is_none());
    assert!(with("http://localhost:3000/api").is_none(), "loopback may be http, as for /mcp");
}
