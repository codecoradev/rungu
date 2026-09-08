//! Official team response endpoint tests (#205).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_auth::session::issue_jwt;
use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::{CurrentUser, PostCategory};
use tower::ServiceExt;

async fn setup_app() -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Test App", "test-app", "A test project").await.unwrap();

    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: "http://localhost:3000".to_string(),
        secure_cookie: false,
        admin_emails: vec!["admin@test.com".to_string()],
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
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-official-test-uploads")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        agent_user_id: std::sync::Arc::new(None),
    };
    let app = axum::Router::new()
        .merge(api_routes().with_state(state.clone()))
        .merge(rungu_api::auth_routes().with_state(state));
    (app, store)
}

async fn seed_post(store: &Store) -> (String, String) {
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let author = store.find_or_create_user("author@test.com", Some("A"), None, &[]).await.unwrap().id;
    let commenter = store.find_or_create_user("commenter@test.com", Some("C"), None, &[]).await.unwrap().id;
    let post = store.create_post(&project.id, "Hot post", "d", PostCategory::Feature, &author).await.unwrap();
    let comment = store.create_comment(&post.id, "Team answer: yes, planned.", None, &commenter).await.unwrap();
    (post.id, comment.comment.id)
}

async fn token(store: &Store, email: &str, secret: &str) -> String {
    // Pass the email itself in admin_emails so an `admin@…` address maps to
    // the admin role (mirrors the find_or_create_user auto-promote branch).
    let admins: Vec<String> = if email.starts_with("admin@") { vec![email.to_string()] } else { vec![] };
    let user = store.find_or_create_user(email, Some("U"), None, &admins).await.unwrap();
    issue_jwt(&CurrentUser { id: user.id.clone(), email: user.email, role: user.role }, secret).unwrap()
}

#[tokio::test]
async fn non_admin_gets_403_even_with_invalid_body() {
    let (app, store) = setup_app().await;
    let (post_id, _) = seed_post(&store).await;
    let member = token(&store, "member@test.com", "test-secret").await;

    // Invalid JSON body + non-admin → must be 403, not 400 (authz before validation).
    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/posts/{post_id}/official-response"))
                .header("cookie", format!("session={member}"))
                .header("content-type", "application/json")
                .body(Body::from("{ not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unauthenticated_gets_401() {
    let (app, store) = setup_app().await;
    let (post_id, _) = seed_post(&store).await;

    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/posts/{post_id}/official-response"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"comment_id": null}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_can_set_and_public_can_read() {
    let (app, store) = setup_app().await;
    let (post_id, comment_id) = seed_post(&store).await;
    let admin = token(&store, "admin@test.com", "test-secret").await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/posts/{post_id}/official-response"))
                .header("cookie", format!("session={admin}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"comment_id": "{comment_id}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["official_response"]["id"], comment_id);
    // No email leaks in the response (#193 rule)
    assert!(!json.to_string().contains("@"));

    // Public read
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/posts/{post_id}/official-response"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["id"], comment_id);
}

#[tokio::test]
async fn cross_post_comment_is_rejected() {
    let (app, store) = setup_app().await;
    let (post_id, _comment_id) = seed_post(&store).await;

    // Comment on a DIFFERENT post
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let author = store.find_or_create_user("author@test.com", Some("A"), None, &[]).await.unwrap().id;
    let other_post = store.create_post(&project.id, "Other post", "d", PostCategory::Feature, &author).await.unwrap();
    let other_comment = store.create_comment(&other_post.id, "From another thread", None, &author).await.unwrap();

    let admin = token(&store, "admin@test.com", "test-secret").await;
    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/posts/{post_id}/official-response"))
                .header("cookie", format!("session={admin}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"comment_id": "{}"}}"#, other_comment.comment.id)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
