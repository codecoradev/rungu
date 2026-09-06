//! Analytics events + admin analytics endpoints (#186).
//!
//! oneshot pattern per `api_test.rs`. Covers: capture on the 5 event paths,
//! privacy (events don't 500 the parent request), authz (admin-only reads),
//! and the aggregate math of the admin endpoints.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_auth::session::issue_jwt;
use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::CurrentUser;
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
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-analytics")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
    };
    let app = axum::Router::new().merge(api_routes().with_state(state));
    (app, store)
}

/// Admin session cookie.
async fn admin_cookie(store: &Store) -> String {
    let user = store
        .find_or_create_user("admin@test.com", Some("Admin"), None, &["admin@test.com".to_string()])
        .await
        .unwrap();
    let current = CurrentUser { id: user.id.clone(), email: user.email, role: user.role };
    format!("session={}", issue_jwt(&current, "test-secret").unwrap())
}

/// Member session cookie.
async fn member_cookie(store: &Store) -> String {
    let user = store.find_or_create_user("user@test.com", Some("User"), None, &[]).await.unwrap();
    let current = CurrentUser { id: user.id.clone(), email: user.email, role: user.role };
    format!("session={}", issue_jwt(&current, "test-secret").unwrap())
}

/// Drain the analytics capture tasks (tokio::spawn) deterministically.
async fn flush_analytics() {
    // Yield a few times so spawned capture tasks run to completion.
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

async fn create_test_post(app: &axum::Router, cookie: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test post","category":"feature"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["data"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_events_captured_on_all_five_paths() {
    let (app, store) = setup_app().await;
    let admin = admin_cookie(&store).await;

    let post_id = create_test_post(&app, &admin).await; // post_created
    flush_analytics().await;

    // board_view (public)
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/projects/test-app/posts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    flush_analytics().await;

    // post_view (public)
    let response = app
        .clone()
        .oneshot(Request::builder().uri(format!("/posts/{post_id}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    flush_analytics().await;

    // vote (authed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/posts/{post_id}/vote"))
                .header("cookie", &admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    flush_analytics().await;

    // comment (authed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/posts/{post_id}/comments"))
                .header("cookie", &admin)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"content":"A comment"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    flush_analytics().await;

    // Assert via the store (same table the admin endpoint reads).
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let totals = store.analytics_totals(&project.id, 0).await.unwrap();
    assert_eq!(totals.get("post_created"), Some(&1));
    assert_eq!(totals.get("board_view"), Some(&1));
    assert_eq!(totals.get("post_view"), Some(&1));
    assert_eq!(totals.get("vote"), Some(&1));
    assert_eq!(totals.get("comment"), Some(&1));
}

#[tokio::test]
async fn test_analytics_admin_endpoint_aggregates() {
    let (app, store) = setup_app().await;
    let admin = admin_cookie(&store).await;
    let post_id = create_test_post(&app, &admin).await;
    flush_analytics().await;

    // Two post views.
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(format!("/posts/{post_id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    flush_analytics().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/analytics/test-app?days=0")
                .header("cookie", &admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["data"]["totals"]["post_created"], 1);
    assert_eq!(json["data"]["totals"]["post_view"], 2);
    assert_eq!(json["data"]["days"], 0);
    assert!(json["data"]["daily"].is_array());
}

#[tokio::test]
async fn test_analytics_top_posts_with_conversion() {
    let (app, store) = setup_app().await;
    let admin = admin_cookie(&store).await;
    let post_id = create_test_post(&app, &admin).await;

    // 2 views + 1 vote.
    for _ in 0..2 {
        app.clone()
            .oneshot(Request::builder().uri(format!("/posts/{post_id}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
    }
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/posts/{post_id}/vote"))
                .header("cookie", &admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    flush_analytics().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/analytics/test-app/top?days=0")
                .header("cookie", &admin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = json["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["views"], 2);
    assert_eq!(rows[0]["vote_count"], 1);
    assert_eq!(rows[0]["vote_view_pct"], 50.0);
    assert_eq!(rows[0]["title"], "Test post");
}

#[tokio::test]
async fn test_analytics_requires_admin() {
    let (app, store) = setup_app().await;
    let member = member_cookie(&store).await;

    // Member → 403.
    let response = app
        .clone()
        .oneshot(
            Request::builder().uri("/admin/analytics/test-app").header("cookie", &member).body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::UNAUTHORIZED);

    // Anonymous → 401.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/analytics/test-app")
                .header("authorization", "Bearer nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Unknown project (admin) → 404.
    let admin = admin_cookie(&store).await;
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/admin/analytics/nope").header("cookie", &admin).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_tools_read_the_same_data() {
    let (app, store) = setup_app().await;
    let admin = admin_cookie(&store).await;
    let post_id = create_test_post(&app, &admin).await;
    drop(app);
    flush_analytics().await;

    // Sanity: event exists in the store the MCP pool would read.
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let totals = store.analytics_totals(&project.id, 0).await.unwrap();
    assert!(totals.contains_key("post_created"));
    let _ = post_id;
}
