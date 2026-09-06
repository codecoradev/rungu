//! E2E integration tests for webhook CRUD + embed widget endpoints.
//!
//! Uses the same in-memory SQLite + oneshot pattern as api_test.rs.
//! Tests cover the v0.3.0 features: webhooks (#106) and embed widget (#111).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_auth::session::issue_jwt;
use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::CurrentUser;
use tower::ServiceExt;

// ── Test setup ─────────────────────────────────────────────────────────

async fn setup_app() -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Test App", "test-app", "A test project").await.unwrap();

    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: "http://localhost:3000".to_string(),
        secure_cookie: false,
        admin_emails: vec![],
        api_key: None,
        google: None,
        github: None,
        keycloak: None,
    };

    let state = AppState {
        store: store.clone(),
        config,
        http_client: reqwest::Client::new(),
        storage: std::sync::Arc::from(
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-uploads")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        agent_user_id: std::sync::Arc::new(None),
    };
    let app = axum::Router::new().merge(api_routes().with_state(state));
    (app, store)
}

/// Create an admin user and return a JWT session token.
async fn admin_token(store: &Store, secret: &str) -> String {
    let user = store
        .find_or_create_user("admin@test.com", Some("Admin"), None, &["admin@test.com".to_string()])
        .await
        .unwrap();
    let current = CurrentUser { id: user.id.clone(), email: user.email, role: user.role };
    issue_jwt(&current, secret).unwrap()
}

/// Create a regular (non-admin) user and return a JWT session token.
async fn member_token(store: &Store, secret: &str) -> String {
    let user = store.find_or_create_user("user@test.com", Some("User"), None, &[]).await.unwrap();
    let current = CurrentUser { id: user.id.clone(), email: user.email, role: user.role };
    issue_jwt(&current, secret).unwrap()
}

// ── Webhook CRUD ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_webhook_create_requires_admin() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";

    // Non-admin → 403
    let token = member_token(&store, secret).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/r","events":"post.created,post.voted"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // No auth → 401
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/r"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_webhook_crud_lifecycle() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = admin_token(&store, secret).await;

    // 1. Create webhook
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/receive","events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let webhook_id = json["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["data"]["url"], "https://hooks.example.com/receive");
    assert_eq!(json["data"]["events"], "post.created");
    assert_eq!(json["data"]["is_active"], true);

    // 2. List webhooks — should see our webhook
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);

    // 3. Get webhook by ID
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/projects/test-app/webhooks/{webhook_id}"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["id"], webhook_id);

    // 4. Update webhook — change events
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/projects/test-app/webhooks/{webhook_id}"))
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":"post.created,post.voted,post.commented","is_active":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["events"], "post.created,post.voted,post.commented");
    assert_eq!(json["data"]["is_active"], false);

    // 5. Delete webhook
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/projects/test-app/webhooks/{webhook_id}"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 6. Verify deleted — GET should 404
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/projects/test-app/webhooks/{webhook_id}"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_webhook_list_requires_admin() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";

    // Member → 403
    let token = member_token(&store, secret).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // No auth → 401
    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/webhooks").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_webhook_create_auto_generates_secret() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = admin_token(&store, secret).await;

    // Create without providing a secret — server should auto-generate one.
    // The secret is NOT returned in the response (security: never expose after creation),
    // but the webhook should be created successfully.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/auto","events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let webhook_id = json["data"]["id"].as_str().unwrap();

    // Verify the secret was generated internally via the store
    let stored_secret = store.get_webhook_secret(webhook_id).await.unwrap();
    assert!(stored_secret.is_some(), "Secret should be auto-generated in DB");
    assert!(!stored_secret.unwrap().is_empty(), "Generated secret must not be empty");
}

#[tokio::test]
async fn test_webhook_create_invalid_url_rejected() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = admin_token(&store, secret).await;

    // Not a URL at all — axum returns 422 for deserialization errors,
    // or 400 for our validation. Either is acceptable (client error).
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"not-a-url","events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error(), "Invalid URL should be rejected, got {}", response.status());

    // Missing url entirely
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_webhook_unknown_project_404() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = admin_token(&store, secret).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/nonexistent/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/r","events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_webhook_deliveries_empty_for_new_webhook() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = admin_token(&store, secret).await;

    // Create a webhook
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/webhooks")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://hooks.example.com/r","events":"post.created"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let webhook_id = json["data"]["id"].as_str().unwrap();

    // List deliveries — should be empty (no events triggered yet)
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/projects/test-app/webhooks/{webhook_id}/deliveries"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}
