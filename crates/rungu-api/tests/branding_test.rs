//! White-label / instance branding tests (#185).
//!
//! Uses the same oneshot pattern as `api_test.rs`. License validation against
//! the real Polar API is NOT tested here (external service) — the badge logic
//! (`LicenseStatus`) is tested directly, which is the unit that matters.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::meta::{InstanceBranding, LicenseInfo, LicenseStatus};
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_core::{Store, open_pool, run_migrations};
use tower::ServiceExt;

/// Build a test app with custom branding + a pre-seeded license status.
async fn setup_branded_app(branding: InstanceBranding, license: LicenseStatus) -> axum::Router {
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
        store,
        config,
        http_client: reqwest::Client::new(),
        storage: std::sync::Arc::from(
            rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-branding")).unwrap(),
        ),
        branding,
        license: std::sync::Arc::new(license),
    };
    axum::Router::new().merge(api_routes().with_state(state))
}

#[tokio::test]
async fn test_meta_defaults_without_branding() {
    let app = setup_branded_app(InstanceBranding::default(), LicenseStatus::new()).await;
    let response = app.oneshot(Request::builder().uri("/meta").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["brandName"], "Rungu");
    assert!(json["logoUrl"].is_null());
    assert_eq!(json["poweredBy"], true);
}

#[tokio::test]
async fn test_meta_returns_custom_branding() {
    let branding = InstanceBranding::new(
        "Acme Corp".to_string(),
        Some("https://acme.test/logo.png".to_string()),
        "Acme feedback".to_string(),
        true,
    );
    let app = setup_branded_app(branding, LicenseStatus::new()).await;
    let response = app.oneshot(Request::builder().uri("/meta").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["brandName"], "Acme Corp");
    assert_eq!(json["logoUrl"], "https://acme.test/logo.png");
    assert_eq!(json["footerText"], "Acme feedback");
    // Unlicensed → badge stays visible regardless of custom branding.
    assert_eq!(json["poweredBy"], true);
}

#[tokio::test]
async fn test_meta_badge_hidden_with_valid_license() {
    let licensed = LicenseStatus::new();
    // No expiry (lifetime tier), checked "recently" (epoch 0 is long past but
    // absence of expiry means never-expiring).
    *licensed.0.write().await = Some(LicenseInfo { licensed: true, checked_at: 0, expires_at: None });
    let app = setup_branded_app(InstanceBranding::default(), licensed).await;
    let response = app.oneshot(Request::builder().uri("/meta").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["poweredBy"], false);
}

#[tokio::test]
async fn test_meta_badge_visible_with_expired_license() {
    let expired = LicenseStatus::new();
    // Expires_at = 1 (unix epoch second 1) → long expired.
    *expired.0.write().await = Some(LicenseInfo { licensed: true, checked_at: 0, expires_at: Some(1) });
    let app = setup_branded_app(InstanceBranding::default(), expired).await;
    let response = app.oneshot(Request::builder().uri("/meta").body(Body::empty()).unwrap()).await.unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Soft gate: expired license → badge reappears, endpoint still 200.
    assert_eq!(json["poweredBy"], true);
}

#[tokio::test]
async fn test_meta_meta_never_requires_auth() {
    // No cookie / Authorization header — must stay public for the embed + SPA.
    let app = setup_branded_app(InstanceBranding::default(), LicenseStatus::new()).await;
    let response = app
        .oneshot(
            Request::builder().uri("/meta").header("authorization", "Bearer forged-token").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_sanitize_strips_control_chars_and_caps_length() {
    let dirty = "Acme\u{0007}\u{001b}[31m Corp".to_string();
    let clean = InstanceBranding::sanitize_config_str(&dirty, 60);
    assert_eq!(clean, "Acme[31m Corp");
    assert_eq!(InstanceBranding::sanitize_config_str(&"x".repeat(100), 10).len(), 10);
}
