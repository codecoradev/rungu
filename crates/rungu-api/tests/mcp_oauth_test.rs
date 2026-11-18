//! OAuth resource-server mode for `/mcp` (#253), against a fake issuer
//! (see `common`).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::http::StatusCode;
use common::*;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rungu_api::mcp_oauth::McpOAuth;
use serde_json::{Value, json};

async fn mcp_call(app: &axum::Router, token: &str, method: &str, params: Value) -> Reply {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    send(
        app,
        "POST",
        "/mcp",
        &[("authorization", &format!("Bearer {token}")), ("content-type", "application/json")],
        &body,
    )
    .await
}

#[tokio::test]
async fn without_an_issuer_the_challenge_is_plain_and_discovery_is_absent() {
    let (app, _) = app_for(None).await;
    let r = send(&app, "POST", "/mcp", &[("content-type", "application/json")], "{}").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.headers["www-authenticate"], "Bearer");
    for path in ["/.well-known/oauth-protected-resource/mcp", "/.well-known/openid-configuration"] {
        assert_eq!(send(&app, "GET", path, &[], "").await.status, StatusCode::NOT_FOUND, "{path}");
    }
    // A JWT-looking token is just a wrong key here.
    assert_eq!(mcp_call(&app, "a.b.c", "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_requests_get_the_resource_metadata_challenge() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;

    let r = send(&app, "POST", "/mcp", &[("content-type", "application/json")], "{}").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        r.headers["www-authenticate"],
        "Bearer resource_metadata=\"http://localhost:3000/.well-known/oauth-protected-resource/mcp\""
    );

    for path in ["/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"] {
        let r = send(&app, "GET", path, &[], "").await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert_eq!(r.json["resource"], RESOURCE);
        assert_eq!(r.json["authorization_servers"][0], issuer.url);
        assert_eq!(r.headers["access-control-allow-origin"], "*");
    }
}

#[tokio::test]
async fn issuer_metadata_is_passed_through_as_json() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    for path in [
        "/.well-known/openid-configuration",
        "/.well-known/oauth-authorization-server",
        "/.well-known/oauth-authorization-server/mcp",
    ] {
        let r = send(&app, "GET", path, &[], "").await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert_eq!(r.json["issuer"], issuer.url);
        assert_eq!(r.json["token_endpoint"], format!("{}/token", issuer.url));
        assert_eq!(r.headers["access-control-allow-origin"], "*");
        assert!(r.headers["content-type"].to_str().unwrap().starts_with("application/json"));
    }
}

#[tokio::test]
async fn a_valid_token_acts_as_the_person_it_names() {
    let issuer = start_issuer().await;
    let (app, store) = app_for(Some(&issuer.url)).await;
    let token = good_token(&issuer);

    let list = mcp_call(&app, &token, "tools/list", json!({})).await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(list.json["result"]["tools"].as_array().unwrap().len() > 20);

    // The token has no email: it was resolved through userinfo, lower-cased,
    // and found-or-created like a web login.
    let alice = store.find_or_create_user("alice@example.test", None, None, &[]).await.unwrap();
    assert_eq!(alice.role, rungu_proto::UserRole::Member);

    // A member can post but is not an admin: no blanket admin for OAuth users.
    let post = mcp_call(
        &app,
        &token,
        "tools/call",
        json!({ "name": "create_post", "arguments": { "slug": "test-app", "title": "From Claude", "category": "feedback" } }),
    )
    .await;
    assert_eq!(post.json["result"]["isError"], false, "{}", post.json);
    assert_eq!(post.json["result"]["structuredContent"]["data"]["created_by"], alice.id);

    let denied =
        mcp_call(&app, &token, "tools/call", json!({ "name": "create_project", "arguments": { "name": "Nope" } }))
            .await;
    assert_eq!(denied.json["result"]["structuredContent"]["error"]["kind"], "forbidden");
}

#[tokio::test]
async fn admin_emails_make_an_oauth_user_an_admin() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    // Email in the token itself (no userinfo needed).
    let mut claims = good_claims(&issuer);
    claims["sub"] = json!("boss-sub");
    claims["email"] = json!(ADMIN);
    claims["email_verified"] = json!(true);
    let token = token_with(&issuer, &issuer.key.der, "k1", claims);

    let made = mcp_call(
        &app,
        &token,
        "tools/call",
        json!({ "name": "create_project", "arguments": { "name": "Admin Board" } }),
    )
    .await;
    assert_eq!(made.json["result"]["isError"], false, "{}", made.json);
}

#[tokio::test]
async fn bad_tokens_are_refused_with_the_challenge() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let other_key = test_key("k1");

    let claims = |edit: &dyn Fn(&mut Value)| {
        let mut c = good_claims(&issuer);
        edit(&mut c);
        c
    };
    let sign = |c: Value| token_with(&issuer, &issuer.key.der, "k1", c);

    let cases: Vec<(&str, String)> = vec![
        ("wrong audience", sign(claims(&|c| c["aud"] = json!("http://localhost:3000/other")))),
        ("wrong issuer", sign(claims(&|c| c["iss"] = json!("http://evil.example")))),
        ("expired", sign(claims(&|c| c["exp"] = json!(now() - 3600)))),
        ("not yet valid", sign(claims(&|c| c["nbf"] = json!(now() + 3600)))),
        (
            "no subject",
            sign(claims(&|c| {
                c.as_object_mut().unwrap().remove("sub");
            })),
        ),
        ("sender constrained", sign(claims(&|c| c["cnf"] = json!({ "jkt": "abc" })))),
        ("forged signature", token_with(&issuer, &other_key.der, "k1", good_claims(&issuer))),
        ("unknown kid", token_with(&issuer, &issuer.key.der, "nope", good_claims(&issuer))),
        ("opaque", "opaque-access-token".to_string()),
        (
            "hs256 with a guessable secret",
            jsonwebtoken::encode(
                &Header::new(Algorithm::HS256),
                &good_claims(&issuer),
                &EncodingKey::from_secret(b"test-secret"),
            )
            .unwrap(),
        ),
    ];
    for (why, token) in cases {
        let r = mcp_call(&app, &token, "tools/list", json!({})).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{why}");
        assert!(r.headers["www-authenticate"].to_str().unwrap().contains("resource_metadata="), "{why}");
        assert!(!r.json.to_string().contains(&token), "{why}: the token is never echoed");
    }
}

#[tokio::test]
async fn an_unverified_email_is_refused() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let mut claims = good_claims(&issuer);
    claims["email"] = json!("x@example.test");
    claims["email_verified"] = json!(false);
    let token = token_with(&issuer, &issuer.key.der, "k1", claims);
    assert_eq!(mcp_call(&app, &token, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_api_key_still_works_and_the_jwks_is_cached() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    assert_eq!(mcp_call(&app, API_KEY, "tools/list", json!({})).await.status, StatusCode::OK);

    let token = good_token(&issuer);
    for _ in 0..3 {
        assert_eq!(mcp_call(&app, &token, "tools/list", json!({})).await.status, StatusCode::OK);
    }
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 1, "keys are fetched once, then cached");

    // An unknown kid doesn't make Rungu hammer the issuer.
    for _ in 0..3 {
        let bad = token_with(&issuer, &issuer.key.der, "rotated", good_claims(&issuer));
        assert_eq!(mcp_call(&app, &bad, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);
    }
    assert!(issuer.jwks_hits.load(Ordering::SeqCst) <= 2, "refetch is throttled");
}

#[tokio::test]
async fn browser_origins_are_checked_and_claude_gets_cors() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;

    // Preflight from claude.ai.
    let r = send(&app, "OPTIONS", "/mcp", &[("origin", "https://claude.ai")], "").await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(r.headers["access-control-allow-origin"], "https://claude.ai");
    let allowed = r.headers["access-control-allow-headers"].to_str().unwrap();
    for h in ["authorization", "content-type", "mcp-protocol-version", "mcp-method", "mcp-name"] {
        assert!(allowed.contains(h), "{h}");
    }

    // The 401 that starts the OAuth flow is readable by the page.
    let r = send(&app, "POST", "/mcp", &[("origin", "https://claude.ai"), ("content-type", "application/json")], "{}")
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.headers["access-control-allow-origin"], "https://claude.ai");
    assert!(r.headers["access-control-expose-headers"].to_str().unwrap().contains("www-authenticate"));

    // Configured extra origin and the app's own origin pass; strangers don't.
    for ok in ["https://inspector.example", "http://localhost:3000"] {
        assert_eq!(send(&app, "OPTIONS", "/mcp", &[("origin", ok)], "").await.status, StatusCode::NO_CONTENT, "{ok}");
    }
    let evil = [("origin", "https://evil.example"), ("authorization", &format!("Bearer {API_KEY}"))];
    assert_eq!(send(&app, "OPTIONS", "/mcp", &evil[..1], "").await.status, StatusCode::FORBIDDEN);
    let r = send(&app, "POST", "/mcp", &evil, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "even a valid key is refused from a foreign origin");
    assert!(r.headers.get("access-control-allow-origin").is_none());

    // No Origin (server-to-server clients) is fine.
    assert_eq!(mcp_call(&app, API_KEY, "tools/list", json!({})).await.status, StatusCode::OK);
}

#[test]
fn misconfiguration_is_rejected_at_startup() {
    let http = reqwest::Client::new();
    let new = |issuer: &str, resource: &str| McpOAuth::new(issuer, resource, vec![], http.clone()).err();
    assert!(new("http://issuer.example", "https://rungu.example/mcp").unwrap().contains("https"));
    assert!(new("https://issuer.example", "http://rungu.example/mcp").unwrap().contains("https"));
    assert!(new("https://issuer.example", "https://rungu.example/elsewhere").unwrap().contains("/mcp"));
    assert!(new("not a url", "https://rungu.example/mcp").is_some());
    assert!(new("https://issuer.example/api/auth/", "https://rungu.example/mcp").is_none());
}

/// Sign a token that names an email itself, for `sub`.
fn email_token(issuer: &Issuer, sub: &str, email: &str, verified: Option<bool>) -> String {
    let mut claims = good_claims(issuer);
    claims["sub"] = json!(sub);
    claims["email"] = json!(email);
    if let Some(v) = verified {
        claims["email_verified"] = json!(v);
    }
    token_with(issuer, &issuer.key.der, "k1", claims)
}

#[tokio::test]
async fn an_email_must_be_marked_verified() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;

    // No claim at all is not "verified": an issuer that lets anyone type an
    // address and omits the flag must not let them claim an admin's.
    let unmarked = email_token(&issuer, "s1", ADMIN, None);
    assert_eq!(mcp_call(&app, &unmarked, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);
    let marked = email_token(&issuer, "s1", ADMIN, Some(true));
    assert_eq!(mcp_call(&app, &marked, "tools/list", json!({})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn built_in_identities_cannot_be_claimed_by_email() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    for email in ["agent@rungu.local", "mcp@rungu.local", "Agent@Rungu.Local"] {
        let token = email_token(&issuer, "s-agent", email, Some(true));
        assert_eq!(mcp_call(&app, &token, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED, "{email}");
    }
}

#[tokio::test]
async fn the_cached_identity_never_overrides_what_a_token_says() {
    let issuer = start_issuer().await;
    let (app, store) = app_for(Some(&issuer.url)).await;

    // First token resolves `sub-x` as a plain member and is cached.
    let first = email_token(&issuer, "sub-x", "member@example.test", Some(true));
    assert_eq!(mcp_call(&app, &first, "tools/list", json!({})).await.status, StatusCode::OK);

    // Same subject within the cache window: a token that now says "unverified"
    // is refused, not served from the cache…
    let unverified = email_token(&issuer, "sub-x", "member@example.test", Some(false));
    assert_eq!(mcp_call(&app, &unverified, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);

    // …and one that names a different email acts as that person.
    let other = email_token(&issuer, "sub-x", ADMIN, Some(true));
    let made = mcp_call(
        &app,
        &other,
        "tools/call",
        json!({ "name": "create_project", "arguments": { "name": "Admin Only" } }),
    )
    .await;
    assert_eq!(made.json["result"]["isError"], false, "{}", made.json);
    let boss = store.find_or_create_user(ADMIN, None, None, &[]).await.unwrap();
    assert_eq!(boss.role, rungu_proto::UserRole::Admin);
}

#[tokio::test]
async fn an_issuer_with_a_trailing_slash_in_iss_works() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;
    let mut claims = good_claims(&issuer);
    claims["iss"] = json!(format!("{}/", issuer.url));
    claims["email"] = json!("slash@example.test");
    claims["email_verified"] = json!(true);
    let token = token_with(&issuer, &issuer.key.der, "k1", claims);
    assert_eq!(mcp_call(&app, &token, "tools/list", json!({})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn an_unreachable_issuer_is_a_503_not_a_reason_to_log_in_again() {
    let (app, _) = app_for(Some("http://127.0.0.1:9")).await;
    let key = test_key("k1");
    let fake = Issuer {
        url: "http://127.0.0.1:9".into(),
        key,
        jwks_hits: Arc::new(AtomicUsize::new(0)),
        revoked: Arc::new(AtomicBool::new(false)),
    };
    let token = good_token(&fake);
    let r = mcp_call(&app, &token, "tools/list", json!({})).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(r.headers.get("www-authenticate").is_none(), "no challenge: the token isn't at fault");
    // The API key doesn't depend on the issuer.
    assert_eq!(mcp_call(&app, API_KEY, "tools/list", json!({})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_refused_token_says_invalid_token_and_the_scheme_is_case_insensitive() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;

    let r = mcp_call(&app, "opaque-token", "tools/list", json!({})).await;
    assert!(r.headers["www-authenticate"].to_str().unwrap().contains("error=\"invalid_token\""));
    let r = send(&app, "POST", "/mcp", &[("content-type", "application/json")], "{}").await;
    assert!(!r.headers["www-authenticate"].to_str().unwrap().contains("invalid_token"), "missing != invalid");

    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string();
    let auth = format!("BEARER {API_KEY}");
    let r = send(&app, "POST", "/mcp", &[("authorization", &auth), ("content-type", "application/json")], &body).await;
    assert_eq!(r.status, StatusCode::OK);
}

/// Gatehouse access tokens are JWTs that cannot be revoked: after a ban the
/// issuer's userinfo answers 401. Rungu must read that as "this token is no
/// longer good" (401, sign in again), not as an outage (503, retry).
#[tokio::test]
async fn a_token_the_issuer_refuses_is_a_401_not_a_503() {
    let issuer = start_issuer().await;
    let (app, _) = app_for(Some(&issuer.url)).await;

    // Works while the person is in good standing.
    let token = good_token(&issuer);
    assert_eq!(mcp_call(&app, &token, "tools/list", json!({})).await.status, StatusCode::OK);

    // Banned: a person we have not resolved yet (no cached identity) is refused.
    issuer.revoked.store(true, Ordering::SeqCst);
    let mut claims = good_claims(&issuer);
    claims["sub"] = json!("banned-sub");
    let banned = token_with(&issuer, &issuer.key.der, "k1", claims);
    let r = mcp_call(&app, &banned, "tools/list", json!({})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "a refused token is not an outage");
    let challenge = r.headers["www-authenticate"].to_str().unwrap();
    assert!(challenge.contains("error=\"invalid_token\"") && challenge.contains("resource_metadata="), "{challenge}");

    // Someone already resolved keeps working only until the cache entry
    // expires (60 s); documented, and the API key is unaffected.
    assert_eq!(mcp_call(&app, API_KEY, "tools/list", json!({})).await.status, StatusCode::OK);
}
