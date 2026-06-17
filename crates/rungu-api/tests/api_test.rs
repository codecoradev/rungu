//! API integration tests — spin up the Axum router and test HTTP endpoints.
//!
//! Uses `tower::ServiceExt::oneshot` to send requests without a real network listener.
//! Each test gets a fresh in-memory SQLite database.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes, auth_routes};
use rungu_auth::AuthConfig;
use rungu_auth::session::issue_jwt;
use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::CurrentUser;
use tower::ServiceExt;

/// Build a test app router with an in-memory database.
async fn setup_app() -> (axum::Router, Store) {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    // Seed a project
    store.create_project("Test App", "test-app", "A test project").await.unwrap();

    let config = AuthConfig {
        app_secret: "test-secret".to_string(),
        app_url: "http://localhost:3000".to_string(),
        secure_cookie: false,
        admin_emails: vec![],
        google: None,
        github: None,
        keycloak: None,
    };

    let state = AppState { store: store.clone(), config, http_client: reqwest::Client::new() };
    // Tests use bare paths (e.g. "/projects") — match the production router structure:
    // API routes under /api, auth routes at root.
    // For simplicity in tests, mount everything at root.
    let app = axum::Router::new().merge(api_routes().with_state(state.clone())).merge(auth_routes().with_state(state));
    (app, store)
}

/// Generate a JWT session token for testing.
fn make_token(user: &CurrentUser, secret: &str) -> String {
    issue_jwt(user, secret).unwrap()
}

/// Create a user in the DB and return a JWT token for them.
async fn authed_user(store: &Store, secret: &str) -> String {
    let user = store.find_or_create_user("user@test.com", Some("Test User"), None, &[]).await.unwrap();
    let current = CurrentUser { id: user.id.clone(), email: user.email, role: user.role };
    make_token(&current, secret)
}

// ── Health & Projects ───────────────────────────────────────────────────

#[tokio::test]
async fn test_list_projects() {
    let (app, _store) = setup_app().await;

    let response = app.oneshot(Request::builder().uri("/projects").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].is_array());
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    assert_eq!(json["data"][0]["slug"], "test-app");
}

#[tokio::test]
async fn test_get_project_by_slug() {
    let (app, _store) = setup_app().await;

    // Existing project
    let response =
        app.clone().oneshot(Request::builder().uri("/projects/test-app").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Non-existent project
    let response =
        app.oneshot(Request::builder().uri("/projects/nonexistent").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_project_requires_admin() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";

    // Non-admin user → 403
    let token = authed_user(&store, secret).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"New Project","slug":"new"}"#))
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
                .uri("/projects")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"No Auth","slug":"noauth"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── Posts ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_posts() {
    let (app, _store) = setup_app().await;

    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/posts").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
    assert_eq!(json["pagination"]["total"], 0);
}

#[tokio::test]
async fn test_create_post_requires_auth() {
    let (app, _store) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test Post"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_create_and_get_post() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = authed_user(&store, secret).await;

    // Create post
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"My Feature Request","description":"Please add dark mode"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let post_id = json["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["data"]["title"], "My Feature Request");

    // Get the post
    let response =
        app.oneshot(Request::builder().uri(format!("/posts/{post_id}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["title"], "My Feature Request");
}

#[tokio::test]
async fn test_create_post_validation() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = authed_user(&store, secret).await;

    // Empty title
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Title too long (>200 chars)
    let long_title = "x".repeat(201);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"title":"{long_title}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_nonexistent_post() {
    let (app, _store) = setup_app().await;

    let response =
        app.oneshot(Request::builder().uri("/posts/nonexistent-id").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Votes ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_vote_requires_auth() {
    let (app, _store) = setup_app().await;

    let response = app
        .oneshot(Request::builder().method("POST").uri("/posts/some-id/vote").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_vote_toggle() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = authed_user(&store, secret).await;

    // Create a post first
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Vote on this"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let post_id = json["data"]["id"].as_str().unwrap();

    // Vote
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/posts/{post_id}/vote"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["voted"], true);
    assert_eq!(json["data"]["vote_count"], 1);

    // Check voted status
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/posts/{post_id}/vote"))
                .header("cookie", format!("session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["voted"], true);
}

// ── Comments ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_create_and_list_comments() {
    let (app, store) = setup_app().await;
    let secret = "test-secret";
    let token = authed_user(&store, secret).await;

    // Create a post
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/test-app/posts")
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Comment on this"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let post_id = json["data"]["id"].as_str().unwrap();

    // Create comment
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/posts/{post_id}/comments"))
                .header("cookie", format!("session={token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"content":"Great post!"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // List comments
    let response = app
        .oneshot(Request::builder().uri(format!("/posts/{post_id}/comments")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    assert_eq!(json["data"][0]["content"], "Great post!");
}

// ── Auth ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_auth_providers_endpoint() {
    let (app, _store) = setup_app().await;

    let response = app.oneshot(Request::builder().uri("/auth/providers").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["providers"].is_array());
    // No providers configured → empty array
    assert_eq!(json["providers"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_posts_invalid_status_returns_400() {
    let (app, _store) = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/projects/test-app/posts?status=opennnn").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // Unknown status filter must be rejected, not silently dropped.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_posts_invalid_category_returns_400() {
    let (app, _store) = setup_app().await;
    let response = app
        .oneshot(
            Request::builder().uri("/projects/test-app/posts?category=notarealcategory").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_posts_empty_result_total_pages_is_at_least_1() {
    let (app, _store) = setup_app().await;
    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/posts").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Empty result must still report a logically-consistent page count (>=1),
    // not 0, to match a 1-based pagination contract.
    assert_eq!(json["pagination"]["total"], 0);
    let total_pages = json["pagination"]["total_pages"].as_i64().unwrap();
    assert!(total_pages >= 1, "total_pages should be at least 1 for empty results, got {total_pages}");
}

#[tokio::test]
async fn test_list_comments_nonexistent_post_returns_404() {
    let (app, _store) = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/posts/nonexistent-id/comments").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // Listing comments for a nonexistent post must 404, not 200 with empty list.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Roadmap view ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_roadmap_unknown_project_returns_404() {
    let (app, _store) = setup_app().await;
    let response =
        app.oneshot(Request::builder().uri("/projects/nope/roadmap").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_roadmap_public_no_auth_required() {
    // Roadmap must be reachable without a session cookie — it's the public
    // status board. If this 401s, the board view for logged-out users breaks.
    let (app, _store) = setup_app().await;
    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/roadmap").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_roadmap_groups_by_status_and_sorts_by_votes() {
    let (app, store) = setup_app().await;
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();

    // Planned bucket: two posts, different vote counts.
    let p1 = store
        .create_post(&project.id, "Planned A", "desc", rungu_proto::PostCategory::Feature, &user.id)
        .await
        .unwrap();
    let p2 = store
        .create_post(&project.id, "Planned B (more votes)", "desc", rungu_proto::PostCategory::Feature, &user.id)
        .await
        .unwrap();
    store.update_post_status(&p1.id, rungu_proto::PostStatus::Planned).await.unwrap();
    store.update_post_status(&p2.id, rungu_proto::PostStatus::Planned).await.unwrap();

    // In-progress bucket.
    let p3 =
        store.create_post(&project.id, "WIP item", "desc", rungu_proto::PostCategory::Bug, &user.id).await.unwrap();
    store.update_post_status(&p3.id, rungu_proto::PostStatus::InProgress).await.unwrap();

    // Done bucket.
    let p4 =
        store.create_post(&project.id, "Shipped", "desc", rungu_proto::PostCategory::Feedback, &user.id).await.unwrap();
    store.update_post_status(&p4.id, rungu_proto::PostStatus::Done).await.unwrap();

    // An OPEN post that must NOT appear on the roadmap (not committed work).
    let _open = store
        .create_post(&project.id, "Just submitted", "desc", rungu_proto::PostCategory::Question, &user.id)
        .await
        .unwrap();

    // Pump p2's votes so it sorts above p1 within the planned bucket.
    let voter = store.find_or_create_user("v@t.com", None, None, &[]).await.unwrap();
    store.toggle_vote(&voter.id, &p2.id).await.unwrap();

    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/roadmap").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let data = &json["data"];
    assert_eq!(data["planned"].as_array().unwrap().len(), 2);
    assert_eq!(data["planned_total"], 2);
    assert_eq!(data["in_progress"].as_array().unwrap().len(), 1);
    assert_eq!(data["in_progress_total"], 1);
    assert_eq!(data["done"].as_array().unwrap().len(), 1);
    assert_eq!(data["done_total"], 1);
    assert_eq!(data["limit"], 10);

    // Within `planned`, the most-voted post ("Planned B") must come first.
    let planned_titles: Vec<&str> =
        data["planned"].as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert_eq!(planned_titles, vec!["Planned B (more votes)", "Planned A"]);
}

#[tokio::test]
async fn test_roadmap_respects_limit_param() {
    let (app, store) = setup_app().await;
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();

    // Create 5 planned posts, ask for a limit of 2.
    for i in 0..5 {
        let p = store
            .create_post(&project.id, &format!("Planned {i}"), "d", rungu_proto::PostCategory::Feature, &user.id)
            .await
            .unwrap();
        store.update_post_status(&p.id, rungu_proto::PostStatus::Planned).await.unwrap();
    }

    let response = app
        .oneshot(Request::builder().uri("/projects/test-app/roadmap?limit=2").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Only 2 posts returned, but the total reflects all 5.
    let data = &json["data"];
    assert_eq!(data["planned"].as_array().unwrap().len(), 2);
    assert_eq!(data["planned_total"], 5);
    assert_eq!(data["limit"], 2);
}

#[tokio::test]
async fn test_roadmap_limit_clamped_to_max_50() {
    // A pathologically large limit must be clamped server-side, not passed
    // straight to SQL — otherwise a client can force a huge result set.
    let (app, _store) = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/projects/test-app/roadmap?limit=9999").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"]["limit"], 50);
}

// ── Changelog ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_changelog_unknown_project_returns_404() {
    let (app, _store) = setup_app().await;
    let response =
        app.oneshot(Request::builder().uri("/projects/nope/changelog").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_changelog_public_no_auth_required() {
    let (app, _store) = setup_app().await;
    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/changelog").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_changelog_returns_only_done_posts() {
    // The changelog must surface ONLY shipped (done) work. Planned, in-progress,
    // and open posts must not leak in.
    let (app, store) = setup_app().await;
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();

    let done_post = store
        .create_post(&project.id, "Shipped feature", "desc", rungu_proto::PostCategory::Feature, &user.id)
        .await
        .unwrap();
    store.update_post_status(&done_post.id, rungu_proto::PostStatus::Done).await.unwrap();

    // These must NOT appear.
    let planned =
        store.create_post(&project.id, "Planned", "desc", rungu_proto::PostCategory::Feature, &user.id).await.unwrap();
    store.update_post_status(&planned.id, rungu_proto::PostStatus::Planned).await.unwrap();
    let wip =
        store.create_post(&project.id, "In progress", "desc", rungu_proto::PostCategory::Bug, &user.id).await.unwrap();
    store.update_post_status(&wip.id, rungu_proto::PostStatus::InProgress).await.unwrap();
    store
        .create_post(&project.id, "Just submitted", "desc", rungu_proto::PostCategory::Question, &user.id)
        .await
        .unwrap();

    let response =
        app.oneshot(Request::builder().uri("/projects/test-app/changelog").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let data = &json["data"];
    assert_eq!(data.as_array().unwrap().len(), 1);
    assert_eq!(data[0]["title"], "Shipped feature");
    assert_eq!(json["pagination"]["total"], 1);
}

#[tokio::test]
async fn test_changelog_invalid_since_returns_400() {
    // A malformed `since` must 400, not silently behave as "no filter" —
    // otherwise a client bug would return the full history unnoticed.
    let (app, _store) = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/projects/test-app/changelog?since=not-a-date").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_changelog_since_filters_out_older_ships() {
    let (app, store) = setup_app().await;
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();

    let p =
        store.create_post(&project.id, "Shipped A", "d", rungu_proto::PostCategory::Feature, &user.id).await.unwrap();
    store.update_post_status(&p.id, rungu_proto::PostStatus::Done).await.unwrap();

    // `since` set to a year in the future → nothing ships after it.
    // The `+` in the RFC3339 offset must be percent-encoded (`%2B`) so it isn't
    // decoded to a space by query-string parsing.
    let future = (chrono::Utc::now() + chrono::Duration::days(365)).to_rfc3339().replace('+', "%2B");
    let uri = format!("/projects/test-app/changelog?since={future}");
    let response = app.oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pagination"]["total"], 0);
    assert!(json["data"].as_array().unwrap().is_empty());
}
