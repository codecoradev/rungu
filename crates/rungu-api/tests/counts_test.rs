//! Public project counts endpoint tests (#202).

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
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-counts-test-uploads")).unwrap(),
        ),
        branding: rungu_api::meta::InstanceBranding::default(),
        license: std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        agent_user_id: std::sync::Arc::new(None),
    };
    let app = axum::Router::new().merge(api_routes().with_state(state));
    (app, store)
}

#[tokio::test]
async fn counts_reflect_status_and_category_distribution() {
    let (app, store) = setup_app().await;
    let project = store.get_project_by_slug("test-app").await.unwrap().unwrap();
    let author = store.find_or_create_user("author@test.com", Some("A"), None, &[]).await.unwrap().id;

    store.create_post(&project.id, "f1", "d", PostCategory::Feature, &author).await.unwrap();
    store.create_post(&project.id, "f2", "d", PostCategory::Feature, &author).await.unwrap();
    store.create_post(&project.id, "b1", "d", PostCategory::Bug, &author).await.unwrap();
    store
        .update_post_status(
            &store
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
                .0
                .iter()
                .find(|p| p.post.title == "f1")
                .unwrap()
                .post
                .id,
            rungu_proto::PostStatus::Planned,
        )
        .await
        .unwrap();

    let res = app
        .oneshot(Request::builder().method("GET").uri("/projects/test-app/counts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let data = &json["data"];
    assert_eq!(data["total"], 3);
    assert_eq!(data["by_status"]["open"], 2);
    assert_eq!(data["by_status"]["planned"], 1);
    assert_eq!(data["by_category"]["feature"], 2);
    assert_eq!(data["by_category"]["bug"], 1);

    // No PII in payload
    let payload = serde_json::to_string(&json).unwrap();
    assert!(!payload.contains("email"), "counts payload must not contain emails");
}

#[tokio::test]
async fn counts_404_for_unknown_project() {
    let (app, _store) = setup_app().await;
    let res = app
        .oneshot(Request::builder().method("GET").uri("/projects/nope/counts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
