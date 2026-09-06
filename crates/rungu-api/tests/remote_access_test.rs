//! AI remote access tests (#189) — ENV API key + MCP over HTTP.
//!
//! oneshot pattern per `api_test.rs`. Covers: Bearer auth on REST (admin
//! power), invalid key 401, keyless 401, and the /mcp JSON-RPC envelope
//! (valid, invalid, missing key).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_core::{Store, open_pool, run_migrations};
use tower::ServiceExt;

const KEY: &str = "test-api-key-0123456789abcdef";

async fn setup_app_with_key() -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Test App", "test-app", "A test project").await.unwrap();

    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: "http://localhost:3000".to_string(),
        secure_cookie: false,
        admin_emails: vec![],
        api_key: Some(KEY.to_string()),
        google: None,
        github: None,
        keycloak: None,
    };

    let state = AppState {
        store: store.clone(),
        config,
        http_client: reqwest::Client::new(),
        storage: std::sync::Arc::from(
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-apikey")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        agent_user_id: std::sync::Arc::new(None),
    };
    let app = axum::Router::new()
        .merge(api_routes().with_state(state.clone()))
        .merge(rungu_api::mcp_http::mcp_routes().with_state(state));
    (app, store)
}

#[tokio::test]
async fn test_bearer_key_wields_admin_power() {
    let (app, _store) = setup_app_with_key().await;

    // Create a project — admin-only action — with just the Bearer key.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects")
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Agent Project"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "API key must wield admin power");

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["data"]["slug"], "agent-project");

    // The action ran under a real user identity? Check via admin stats.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/analytics/agent-project")
                .header("authorization", format!("Bearer {KEY}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "Bearer key must reach admin endpoints");
}

#[tokio::test]
async fn test_bearer_invalid_key_never_escalates() {
    let (app, _store) = setup_app_with_key().await;

    // Admin action (CurrentUser extractor) with a bad key → 401. A session
    // cookie must NOT rescue a bad Bearer key (no confusion attacks).
    for cookie in [None, Some("session=forged-token")] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/projects")
            .header("authorization", "Bearer wrong-key-aaaaaaaaaaaaaaaa")
            .header("content-type", "application/json");
        if let Some(c) = cookie {
            builder = builder.header("cookie", c);
        }
        let response =
            app.clone().oneshot(builder.body(Body::from(r#"{"name":"Should Not Exist"}"#)).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "bad key must never gain admin power");
    }

    // Public reads (OptionalCurrentUser) treat a bad key as anonymous — 200
    // public data, never privileged.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/projects")
                .header("authorization", "Bearer wrong-key-aaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/projects/test-app/stats")
                .header("authorization", "Bearer wrong-key-aaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "bad key must not reach admin endpoints");
}

#[tokio::test]
async fn test_no_key_no_session_still_401() {
    let (app, _store) = setup_app_with_key().await;
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/admin/projects/test-app/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_http_envelope_and_auth() {
    let (app, _store) = setup_app_with_key().await;

    // Valid key → JSON-RPC result.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"list_projects","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["id"], 1);
    assert!(json["result"]["data"].is_array(), "expected data array envelope");

    // A tool call with params works too (analytics on known project).
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {KEY}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"jsonrpc":"2.0","id":2,"method":"get_analytics","params":{"slug":"test-app","days":0}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["result"]["data"]["project"], "test-app");

    // Missing key → 401.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":3,"method":"list_projects"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Wrong key → 401.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", "Bearer nope-nope-nope")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":4,"method":"list_projects"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_constant_time_compare_rejects_wrong_lengths() {
    // Indirect: the helper is private; behavior is covered by the 401 tests
    // above (wrong-length keys). This test documents intent.
    let config = AuthConfig {
        app_secret: "s".into(),
        app_url: "http://localhost:3000".into(),
        secure_cookie: false,
        admin_emails: vec![],
        api_key: Some("short".into()),
        google: None,
        github: None,
        keycloak: None,
    };
    assert!(!rungu_auth::middleware::verify_api_key(&config, "longer-key-value"));
    assert!(!rungu_auth::middleware::verify_api_key(&config, ""));
    assert!(rungu_auth::middleware::verify_api_key(&config, "short"));
}
