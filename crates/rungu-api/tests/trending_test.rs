//! Trending sort integration tests (#203).
//!
//! Trending = 7-day vote velocity from `analytics_events`, tie-broken by
//! total votes; posts without events fall back to vote totals. Uses the
//! oneshot router pattern (see api_test.rs) with a fresh in-memory SQLite DB.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::PostCategory;
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
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-trending-test-uploads")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        agent_user_id: std::sync::Arc::new(None),
    };
    let app = axum::Router::new().merge(api_routes().with_state(state));
    (app, store)
}

async fn user(store: &Store, email: &str) -> String {
    store.find_or_create_user(email, Some("Tester"), None, &[]).await.unwrap().id
}

async fn list_titles(app: &axum::Router, sort: &str) -> Vec<String> {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/projects/test-app/posts?sort={sort}&per_page=50"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["data"].as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap().to_string()).collect()
}

/// Seeded fixture:
/// - "hot"  — velocity 5 (5 vote events), total votes 1
/// - "big"  — velocity 1 (1 vote event),  total votes 5
/// - "cold" — no events, no votes
async fn seed(store: &Store) {
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let author = user(store, "author@test.com").await;

    let hot = store.create_post(&project.id, "hot", "d", PostCategory::Feature, &author).await.unwrap();
    let big = store.create_post(&project.id, "big", "d", PostCategory::Feature, &author).await.unwrap();
    store.create_post(&project.id, "cold", "d", PostCategory::Feature, &author).await.unwrap();

    // Vote events (velocity): 5 for hot, 1 for big.
    for _ in 0..5 {
        store.record_event(&project.id, Some(&hot.id), "vote").await.unwrap();
    }
    store.record_event(&project.id, Some(&big.id), "vote").await.unwrap();

    // Total votes: 1 for hot, 5 for big (one row per real user).
    let v1 = user(store, "v1@test.com").await;
    store.toggle_vote(&v1, &hot.id).await.unwrap();
    for i in 2..7 {
        let v = user(store, &format!("v{i}@test.com")).await;
        store.toggle_vote(&v, &big.id).await.unwrap();
    }
}

#[tokio::test]
async fn trending_orders_by_velocity_beating_total_votes() {
    let (app, store) = setup_app().await;
    seed(&store).await;

    let trending = list_titles(&app, "trending").await;
    assert_eq!(trending, vec!["hot".to_string(), "big".to_string(), "cold".to_string()]);

    // Sanity: the same data under most_votes orders by totals instead.
    let most = list_titles(&app, "most_votes").await;
    assert_eq!(most, vec!["big".to_string(), "hot".to_string(), "cold".to_string()]);
}

#[tokio::test]
async fn trending_falls_back_to_totals_when_no_events_exist() {
    let (app, store) = setup_app().await;
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let author = user(&store, "author@test.com").await;
    store.create_post(&project.id, "plain-a", "d", PostCategory::Feature, &author).await.unwrap();
    store.create_post(&project.id, "plain-b", "d", PostCategory::Feature, &author).await.unwrap();

    let v1 = user(&store, "v1@test.com").await;
    let posts = store
        .list_posts(rungu_proto::ListPostsParams {
            project_id: &project.id,
            sort: rungu_proto::PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            user_id: None,
            offset: 0,
            limit: 50,
        })
        .await
        .unwrap()
        .0;
    let b_id = posts.iter().find(|p| p.post.title == "plain-b").unwrap().post.id.clone();
    store.toggle_vote(&v1, &b_id).await.unwrap();

    let titles = list_titles(&app, "trending").await;
    // Both posts have zero events → tie-break on vote_count (b has 1, a has 0).
    assert_eq!(titles, vec!["plain-b".to_string(), "plain-a".to_string()]);
}
