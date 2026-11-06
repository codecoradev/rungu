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

    let state = AppState::new(
        store.clone(),
        config,
        reqwest::Client::new(),
        std::sync::Arc::from(rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-apikey")).unwrap()),
        rungu_api::meta::InstanceBranding::default(),
        std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        std::sync::Arc::new(None),
    );
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

/// #248: the transport decides who is acting. Over `/mcp` a valid key is the
/// same `ai-agent` identity REST gives it, so rows carry that author (not a
/// separate `mcp@rungu.local` user), and failures carry a code per kind.
#[tokio::test]
async fn test_mcp_http_acts_as_the_api_key_agent_and_codes_errors() {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    let store = Store::new_with_kind(pool, true);
    store.create_project("Test App", "test-app", "").await.unwrap();
    let agent = store
        .find_or_create_user(
            rungu_auth::middleware::AGENT_USER_EMAIL,
            Some(rungu_auth::middleware::AGENT_USER_NAME),
            None,
            &[rungu_auth::middleware::AGENT_USER_EMAIL.to_string()],
        )
        .await
        .unwrap();

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
    let state = AppState::new(
        store.clone(),
        config,
        reqwest::Client::new(),
        std::sync::Arc::from(rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-test-apikey-agent")).unwrap()),
        rungu_api::meta::InstanceBranding::default(),
        std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
        std::sync::Arc::new(Some(agent.id.clone())),
    );
    let app = axum::Router::new().merge(rungu_api::mcp_http::mcp_routes().with_state(state));

    let call = |body: &'static str| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/mcp")
                        .header("authorization", format!("Bearer {KEY}"))
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "JSON-RPC errors ride on HTTP 200");
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        }
    };

    let created =
        call(r#"{"jsonrpc":"2.0","id":1,"method":"create_post","params":{"slug":"test-app","title":"From an agent"}}"#)
            .await;
    assert_eq!(created["result"]["data"]["created_by"], agent.id, "authored by the API key's agent identity");

    let not_found = call(r#"{"jsonrpc":"2.0","id":2,"method":"get_project","params":{"slug":"nope"}}"#).await;
    assert_eq!(not_found["error"]["code"], -32004);
    assert_eq!(not_found["error"]["data"]["kind"], "not_found");

    let bad =
        call(r#"{"jsonrpc":"2.0","id":3,"method":"create_project","params":{"name":"X","slug":"Bad Slug"}}"#).await;
    assert_eq!(bad["error"]["code"], -32602);
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

/// One `/mcp` call with the key and the given extra headers.
async fn mcp(
    app: &axum::Router,
    method: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri("/mcp")
        .header("authorization", format!("Bearer {KEY}"))
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let response = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

const MODERN_META: &str = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}"#;

/// The stateless 2026-07-28 flow a Claude connector uses: discover, list,
/// call — no `initialize`, no session id.
#[tokio::test]
async fn test_mcp_http_stateless_flow() {
    let (app, _store) = setup_app_with_key().await;
    let modern = [("mcp-protocol-version", "2026-07-28")];

    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{{{MODERN_META}}}}}"#);
    let (status, json) = mcp(&app, "POST", &[modern[0], ("mcp-method", "server/discover")], &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["supportedVersions"][0], "2026-07-28");

    let body = format!(r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{{MODERN_META}}}}}"#);
    let (_, json) = mcp(&app, "POST", &[modern[0], ("mcp-method", "tools/list")], &body).await;
    assert_eq!(json["result"]["tools"].as_array().unwrap().len(), rungu_mcp::TOOLS.len());
    assert_eq!(json["result"]["resultType"], "complete");

    let body = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"list_projects","arguments":{{}},{MODERN_META}}}}}"#
    );
    let headers = [modern[0], ("mcp-method", "tools/call"), ("mcp-name", "list_projects")];
    let (status, json) = mcp(&app, "POST", &headers, &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["isError"], false);
    assert!(json["result"]["structuredContent"]["data"].is_array());
}

#[tokio::test]
async fn test_mcp_http_rejects_mismatched_headers() {
    let (app, _store) = setup_app_with_key().await;
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"list_projects",{MODERN_META}}}}}"#
    );
    let v = ("mcp-protocol-version", "2026-07-28");

    for headers in [
        vec![v],                                                               // no Mcp-Method
        vec![v, ("mcp-method", "tools/list"), ("mcp-name", "list_projects")],  // wrong method
        vec![v, ("mcp-method", "tools/call")],                                 // no Mcp-Name
        vec![v, ("mcp-method", "tools/call"), ("mcp-name", "delete_project")], // wrong name
    ] {
        let (status, json) = mcp(&app, "POST", &headers, &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{headers:?}");
        assert_eq!(json["error"]["code"], -32020);
        assert_eq!(json["id"], 9);
    }

    let (status, json) = mcp(&app, "POST", &[("mcp-protocol-version", "2031-01-01")], &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], -32022);
    assert_eq!(json["error"]["data"]["supported"][0], "2026-07-28");
}

#[tokio::test]
async fn test_mcp_http_legacy_handshake_notifications_and_methods() {
    let (app, _store) = setup_app_with_key().await;

    let (status, json) = mcp(
        &app,
        "POST",
        &[("accept", "application/json, text/event-stream")],
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"]["protocolVersion"], "2025-06-18");

    // A notification gets 202 and no body.
    let (status, json) = mcp(&app, "POST", &[], r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(json.is_null());

    let (_, json) = mcp(
        &app,
        "POST",
        &[],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_project","arguments":{"slug":"nope"}}}"#,
    )
    .await;
    assert_eq!(json["result"]["isError"], true);

    // A tool call sent without an id is never executed.
    let (status, _) = mcp(
        &app,
        "POST",
        &[],
        r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"create_project","arguments":{"name":"Ghost"}}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (_, json) =
        mcp(&app, "POST", &[], r#"{"jsonrpc":"2.0","id":3,"method":"get_project","params":{"slug":"ghost"}}"#).await;
    assert_eq!(json["error"]["kind"].as_str().or(json["error"]["data"]["kind"].as_str()), Some("not_found"));

    // Batches are refused, not silently dropped.
    let (status, json) = mcp(&app, "POST", &[], r#"[{"jsonrpc":"2.0","id":1,"method":"tools/list"}]"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], -32600);

    // No server-initiated streams and no sessions.
    for method in ["GET", "DELETE"] {
        let (status, _) = mcp(&app, method, &[], "").await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
}
