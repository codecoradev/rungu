//! REST ↔ MCP parity (#249).
//!
//! Two kinds of check, both driven by the tool catalog in `rungu-mcp`
//! (`TOOLS`, `REST_ONLY`):
//!
//! 1. **Coverage** — every operation in the OpenAPI document is claimed by a
//!    tool's REST route or listed as REST-only with a reason, and the access
//!    level a tool declares agrees with the route's security. Add an endpoint
//!    or a tool and skip the catalog, and this fails.
//! 2. **Behaviour** — one scenario runs the same capability through the real
//!    REST router and through MCP on the same database, and both must end the
//!    same way: ok, invalid, forbidden, not found or conflict. Reads must also
//!    return the same data.

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rungu_api::openapi::ApiDoc;
use rungu_api::{AppState, api_routes};
use rungu_auth::AuthConfig;
use rungu_auth::session::issue_jwt;
use rungu_core::{Actor, Operations, Store, open_pool, run_migrations};
use rungu_mcp::{Access, REST_ONLY, TOOLS};
use rungu_proto::{CurrentUser, UserRole};
use serde_json::{Value, json};
use tower::ServiceExt;
use utoipa::OpenApi;

// ── Coverage ─────────────────────────────────────────────────────────────

/// Every (METHOD, path, has_security) in the OpenAPI document.
fn openapi_operations() -> Vec<(String, String, bool)> {
    let doc = ApiDoc::openapi();
    let mut ops = Vec::new();
    for (path, item) in &doc.paths.paths {
        for (method, op) in [
            ("GET", &item.get),
            ("POST", &item.post),
            ("PUT", &item.put),
            ("PATCH", &item.patch),
            ("DELETE", &item.delete),
        ] {
            if let Some(op) = op {
                let secured = op.security.as_ref().is_some_and(|s| !s.is_empty());
                ops.push((method.to_string(), path.clone(), secured));
            }
        }
    }
    ops
}

#[test]
fn every_rest_operation_has_a_tool_or_a_stated_reason() {
    let documented: BTreeSet<(String, String)> = openapi_operations().into_iter().map(|(m, p, _)| (m, p)).collect();
    let claimed: BTreeSet<(String, String)> =
        TOOLS.iter().map(|t| (t.rest.method.to_string(), t.rest.path.to_string())).collect();
    let rest_only: BTreeSet<(String, String)> =
        REST_ONLY.iter().map(|(m, p, _)| (m.to_string(), p.to_string())).collect();

    let unclaimed: Vec<_> = documented.difference(&claimed).filter(|op| !rest_only.contains(*op)).collect();
    assert!(
        unclaimed.is_empty(),
        "REST operations with no MCP tool and no REST_ONLY entry (add a tool in rungu-mcp/src/tools.rs, or list \
         them in REST_ONLY with a reason): {unclaimed:?}"
    );

    let phantom_tool_routes: Vec<_> = claimed.difference(&documented).collect();
    assert!(
        phantom_tool_routes.is_empty(),
        "tools point at REST operations that aren't in the OpenAPI document: {phantom_tool_routes:?}"
    );
    let phantom_rest_only: Vec<_> = rest_only.difference(&documented).collect();
    assert!(phantom_rest_only.is_empty(), "REST_ONLY lists operations that no longer exist: {phantom_rest_only:?}");
}

#[test]
fn declared_access_agrees_with_the_route_security() {
    let secured: std::collections::BTreeMap<(String, String), bool> =
        openapi_operations().into_iter().map(|(m, p, s)| ((m, p), s)).collect();
    for tool in TOOLS {
        let route_is_secured = secured[&(tool.rest.method.to_string(), tool.rest.path.to_string())];
        assert_eq!(
            tool.access != Access::Public,
            route_is_secured,
            "{}: declares {:?} but {} {} is {}",
            tool.name,
            tool.access,
            tool.rest.method,
            tool.rest.path,
            if route_is_secured { "secured" } else { "public" }
        );
    }
}

// ── Behaviour ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ok,
    Invalid,
    Forbidden,
    NotFound,
    Conflict,
}

struct Reply {
    outcome: Outcome,
    /// REST: the JSON body (Null for 204). MCP: the `result`.
    body: Value,
}

struct Role {
    actor: Actor,
    token: String,
}

struct World {
    app: axum::Router,
    ops: Operations,
    admin: Role,
    owner: Role,
    stranger: Role,
    /// Every MCP tool a scenario has called, so a test can require that each
    /// tool in the catalog is exercised by at least one scenario.
    called: std::sync::Mutex<BTreeSet<String>>,
    /// Uploads directory; removed when the world is dropped.
    _uploads: tempfile::TempDir,
}

impl World {
    async fn new() -> Self {
        let pool = open_pool("sqlite::memory:").await.unwrap();
        run_migrations(&pool, "sqlite::memory:").await.unwrap();
        let store = Store::new_with_kind(pool, true);
        store.create_project("Test App", "test-app", "A test project").await.unwrap();

        let uploads = tempfile::tempdir().unwrap();
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
        let state = AppState::new(
            store.clone(),
            config,
            reqwest::Client::new(),
            std::sync::Arc::from(rungu_core::FsStorage::new(uploads.path().to_path_buf()).unwrap()),
            rungu_api::meta::InstanceBranding::default(),
            std::sync::Arc::new(rungu_api::meta::LicenseStatus::new()),
            std::sync::Arc::new(None),
        );
        let ops = state.ops.clone();
        // Mounted under /api, as in production, so the OpenAPI paths work as-is.
        let app = axum::Router::new().nest("/api", api_routes().with_state(state));

        let role = |email: &'static str, role: UserRole| {
            let store = store.clone();
            async move {
                let user = store.find_or_create_user(email, None, None, &[]).await.unwrap();
                let token =
                    issue_jwt(&CurrentUser { id: user.id.clone(), email: user.email, role }, "test-secret").unwrap();
                Role { actor: Actor { id: user.id, role }, token }
            }
        };
        Self {
            admin: role("admin@parity.test", UserRole::Admin).await,
            owner: role("owner@parity.test", UserRole::Member).await,
            stranger: role("stranger@parity.test", UserRole::Member).await,
            app,
            ops,
            called: Default::default(),
            _uploads: uploads,
        }
    }

    fn called_tools(&self) -> BTreeSet<String> {
        self.called.lock().unwrap().clone()
    }

    async fn rest(&self, who: &Role, method: &str, uri: &str, body: Option<Value>) -> Reply {
        let mut req = Request::builder().method(method).uri(uri).header("cookie", format!("session={}", who.token));
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let response = self.app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let outcome = match status {
            StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => Outcome::Ok,
            StatusCode::BAD_REQUEST => Outcome::Invalid,
            StatusCode::FORBIDDEN => Outcome::Forbidden,
            StatusCode::NOT_FOUND => Outcome::NotFound,
            StatusCode::CONFLICT => Outcome::Conflict,
            other => panic!("REST {method} {uri} → unexpected status {other}: {}", String::from_utf8_lossy(&bytes)),
        };
        Reply { outcome, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
    }

    async fn mcp(&self, who: &Role, tool: &str, params: Value) -> Reply {
        self.called.lock().unwrap().insert(tool.to_string());
        let input = json!({ "jsonrpc": "2.0", "method": tool, "params": params, "id": 1 }).to_string();
        let out: Value = serde_json::from_str(&rungu_mcp::handle_message(&input, &self.ops, &who.actor).await).unwrap();
        if let Some(code) = out["error"]["code"].as_i64() {
            let outcome = match code {
                -32602 => Outcome::Invalid,
                -32003 => Outcome::Forbidden,
                -32004 => Outcome::NotFound,
                -32009 => Outcome::Conflict,
                other => panic!("MCP {tool} → unexpected error {other}: {}", out["error"]["message"]),
            };
            Reply { outcome, body: Value::Null }
        } else {
            Reply { outcome: Outcome::Ok, body: out["result"].clone() }
        }
    }

    /// Run one capability through REST and through MCP as the same role, and
    /// require both to end as `expected`.
    async fn both(
        &self,
        who: &Role,
        step: &str,
        rest: (&str, &str, Option<Value>),
        mcp: (&str, Value),
        expected: Outcome,
    ) -> (Reply, Reply) {
        let r = self.rest(who, rest.0, rest.1, rest.2).await;
        let m = self.mcp(who, mcp.0, mcp.1).await;
        assert_eq!(r.outcome, expected, "{step}: REST {} {}", rest.0, rest.1);
        assert_eq!(m.outcome, expected, "{step}: MCP {}", mcp.0);
        (r, m)
    }
}

fn id_of(v: &Value) -> String {
    v["data"]["id"].as_str().or_else(|| v["data"]["comment"]["id"].as_str()).expect("an id in the reply").to_string()
}

fn ids(posts: &Value) -> Vec<&str> {
    posts.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn projects_behave_the_same_over_rest_and_mcp() {
    scenario_projects(&World::new().await).await;
}

async fn scenario_projects(w: &World) {
    use Outcome::*;

    w.rest(&w.admin, "POST", "/api/projects", Some(json!({ "name": "From REST", "slug": "from-rest" }))).await;
    w.mcp(&w.admin, "create_project", json!({ "name": "From MCP", "slug": "from-mcp" })).await;
    for slug in ["from-rest", "from-mcp"] {
        let (r, _) = w
            .both(
                &w.admin,
                "read",
                ("GET", &format!("/api/projects/{slug}"), None),
                ("get_project", json!({ "slug": slug })),
                Ok,
            )
            .await;
        assert_eq!(r.body["data"]["slug"], slug);
    }

    let (r, m) =
        w.both(&w.admin, "list projects", ("GET", "/api/projects", None), ("list_projects", json!({})), Ok).await;
    let slugs = |v: &Value| -> BTreeSet<String> {
        v.as_array().unwrap().iter().map(|p| p["slug"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(slugs(&r.body["data"]), slugs(&m.body["data"]));
    assert!(slugs(&m.body["data"]).contains("from-mcp"));

    let dup = json!({ "name": "Again", "slug": "from-rest" });
    w.both(&w.admin, "duplicate slug", ("POST", "/api/projects", Some(dup.clone())), ("create_project", dup), Conflict)
        .await;
    let bad = json!({ "name": "Bad", "slug": "Has Spaces" });
    w.both(&w.admin, "invalid slug", ("POST", "/api/projects", Some(bad.clone())), ("create_project", bad), Invalid)
        .await;
    let member = json!({ "name": "Nope" });
    w.both(
        &w.owner,
        "member creating",
        ("POST", "/api/projects", Some(member.clone())),
        ("create_project", member),
        Forbidden,
    )
    .await;
    w.both(
        &w.admin,
        "unknown project",
        ("GET", "/api/projects/nope", None),
        ("get_project", json!({ "slug": "nope" })),
        NotFound,
    )
    .await;

    w.both(
        &w.owner,
        "member deleting",
        ("DELETE", "/api/projects/from-rest", None),
        ("delete_project", json!({ "slug": "from-rest" })),
        Forbidden,
    )
    .await;
    w.rest(&w.admin, "DELETE", "/api/projects/from-rest", None).await;
    w.mcp(&w.admin, "delete_project", json!({ "slug": "from-mcp" })).await;
    for slug in ["from-rest", "from-mcp"] {
        w.both(
            &w.admin,
            "deleted twice",
            ("DELETE", &format!("/api/projects/{slug}"), None),
            ("delete_project", json!({ "slug": slug })),
            NotFound,
        )
        .await;
    }
}

#[tokio::test]
async fn posts_votes_and_comments_behave_the_same_over_rest_and_mcp() {
    scenario_posts(&World::new().await).await;
}

async fn scenario_posts(w: &World) {
    use Outcome::*;

    // Create: one post per transport, same author.
    let rest_post = w
        .rest(
            &w.owner,
            "POST",
            "/api/projects/test-app/posts",
            Some(json!({ "title": "REST post", "category": "bug" })),
        )
        .await;
    assert_eq!(rest_post.outcome, Ok);
    let mcp_post =
        w.mcp(&w.owner, "create_post", json!({ "slug": "test-app", "title": "MCP post", "category": "bug" })).await;
    assert_eq!(mcp_post.outcome, Ok);
    let (rest_id, mcp_id) = (id_of(&rest_post.body), id_of(&mcp_post.body));

    let bad_category = json!({ "title": "x", "category": "nonsense" });
    w.both(
        &w.owner,
        "invalid category",
        ("POST", "/api/projects/test-app/posts", Some(bad_category.clone())),
        ("create_post", json!({ "slug": "test-app", "title": "x", "category": "nonsense" })),
        Invalid,
    )
    .await;
    w.both(
        &w.owner,
        "blank title",
        ("POST", "/api/projects/test-app/posts", Some(json!({ "title": "  " }))),
        ("create_post", json!({ "slug": "test-app", "title": "  " })),
        Invalid,
    )
    .await;
    w.both(
        &w.owner,
        "unknown project",
        ("POST", "/api/projects/nope/posts", Some(json!({ "title": "x" }))),
        ("create_post", json!({ "slug": "nope", "title": "x" })),
        NotFound,
    )
    .await;

    // Reads return the same data.
    let (r, m) = w
        .both(
            &w.owner,
            "get post",
            ("GET", &format!("/api/posts/{rest_id}"), None),
            ("get_post", json!({ "id": rest_id })),
            Ok,
        )
        .await;
    assert_eq!(r.body["data"]["id"], m.body["data"]["id"]);
    assert_eq!(r.body["data"]["title"], m.body["data"]["title"]);

    // What the viewer may do with the post is part of the read, and both
    // transports report the same flags for the same person (#264).
    for (role, can_edit, can_moderate) in [(&w.owner, true, false), (&w.admin, true, true), (&w.stranger, false, false)]
    {
        let (r, m) = w
            .both(
                role,
                "post permissions",
                ("GET", &format!("/api/posts/{rest_id}"), None),
                ("get_post", json!({ "id": rest_id })),
                Ok,
            )
            .await;
        let expected = json!({ "can_edit": can_edit, "can_moderate": can_moderate });
        assert_eq!(r.body["data"]["permissions"], expected, "REST, {:?}", role.actor.role);
        assert_eq!(m.body["data"]["permissions"], expected, "MCP, {:?}", role.actor.role);
    }

    w.both(&w.owner, "unknown post", ("GET", "/api/posts/nope", None), ("get_post", json!({ "id": "nope" })), NotFound)
        .await;
    let (r, m) = w
        .both(
            &w.owner,
            "list posts",
            ("GET", "/api/projects/test-app/posts?sort=oldest&per_page=10", None),
            ("list_posts", json!({ "slug": "test-app", "sort": "oldest", "limit": 10 })),
            Ok,
        )
        .await;
    assert_eq!(ids(&r.body["data"]), ids(&m.body["data"]));
    assert_eq!(r.body["pagination"]["total"], m.body["total"]);
    w.both(
        &w.owner,
        "filter typo",
        ("GET", "/api/projects/test-app/posts?status=opennn", None),
        ("list_posts", json!({ "slug": "test-app", "status": "opennn" })),
        Invalid,
    )
    .await;

    let (r, m) = w
        .both(
            &w.owner,
            "search",
            ("GET", "/api/projects/test-app/posts?q=REST", None),
            ("search_posts", json!({ "slug": "test-app", "q": "REST" })),
            Ok,
        )
        .await;
    assert_eq!(ids(&r.body["data"]), ids(&m.body["data"]));
    assert_eq!(ids(&m.body["data"]), vec![rest_id.as_str()], "only the REST post matches");
    let (r, m) = w
        .both(
            &w.owner,
            "trending",
            ("GET", "/api/projects/test-app/posts?sort=trending&per_page=10", None),
            ("get_trending", json!({ "slug": "test-app", "limit": 10 })),
            Ok,
        )
        .await;
    assert_eq!(ids(&r.body["data"]), ids(&m.body["data"]));

    // Updates: author or admin only; input validated.
    for (step, patch, tool, params) in [
        ("status", json!({ "status": "planned" }), "update_post_status", json!({ "id": rest_id, "status": "planned" })),
        (
            "category",
            json!({ "category": "feature" }),
            "update_post_category",
            json!({ "id": rest_id, "category": "feature" }),
        ),
    ] {
        let uri = format!("/api/posts/{rest_id}");
        w.both(&w.owner, step, ("PATCH", &uri, Some(patch.clone())), (tool, params.clone()), Ok).await;
        w.both(&w.stranger, step, ("PATCH", &uri, Some(patch)), (tool, params), Forbidden).await;
    }
    w.both(
        &w.owner,
        "invalid status",
        ("PATCH", &format!("/api/posts/{rest_id}"), Some(json!({ "status": "nope" }))),
        ("update_post_status", json!({ "id": rest_id, "status": "nope" })),
        Invalid,
    )
    .await;
    w.both(
        &w.owner,
        "unknown post",
        ("PATCH", "/api/posts/nope", Some(json!({ "status": "done" }))),
        ("update_post_status", json!({ "id": "nope", "status": "done" })),
        NotFound,
    )
    .await;

    // Roadmap shows the planned post identically.
    let (r, m) = w
        .both(
            &w.owner,
            "roadmap",
            ("GET", "/api/projects/test-app/roadmap?limit=5", None),
            ("get_roadmap", json!({ "slug": "test-app", "limit": 5 })),
            Ok,
        )
        .await;
    assert_eq!(r.body["data"], m.body["data"]);

    // Votes toggle for anyone logged in.
    let (r, m) = w
        .both(
            &w.stranger,
            "vote",
            ("POST", &format!("/api/posts/{rest_id}/vote"), None),
            ("vote_post", json!({ "id": rest_id })),
            Ok,
        )
        .await;
    assert_ne!(
        r.body["data"]["voted"], m.body["data"]["voted"],
        "the same voter toggling twice adds the vote, then removes it"
    );
    w.both(
        &w.stranger,
        "vote on unknown",
        ("POST", "/api/posts/nope/vote", None),
        ("vote_post", json!({ "id": "nope" })),
        NotFound,
    )
    .await;

    // Comments: validated, and only the author or an admin may delete.
    let rest_comment = w
        .rest(&w.stranger, "POST", &format!("/api/posts/{rest_id}/comments"), Some(json!({ "content": "from REST" })))
        .await;
    let mcp_comment = w.mcp(&w.stranger, "add_comment", json!({ "post_id": rest_id, "content": "from MCP" })).await;
    assert_eq!((rest_comment.outcome, mcp_comment.outcome), (Ok, Ok));
    let (rest_cid, mcp_cid) = (id_of(&rest_comment.body), id_of(&mcp_comment.body));
    w.both(
        &w.stranger,
        "blank comment",
        ("POST", &format!("/api/posts/{rest_id}/comments"), Some(json!({ "content": " " }))),
        ("add_comment", json!({ "post_id": rest_id, "content": " " })),
        Invalid,
    )
    .await;
    w.both(
        &w.stranger,
        "comment on unknown post",
        ("POST", "/api/posts/nope/comments", Some(json!({ "content": "x" }))),
        ("add_comment", json!({ "post_id": "nope", "content": "x" })),
        NotFound,
    )
    .await;
    let (r, m) = w
        .both(
            &w.owner,
            "list comments",
            ("GET", &format!("/api/posts/{rest_id}/comments"), None),
            ("list_comments", json!({ "post_id": rest_id })),
            Ok,
        )
        .await;
    assert_eq!(r.body["data"].as_array().unwrap().len(), m.body["data"].as_array().unwrap().len());
    w.both(
        &w.owner,
        "comments of unknown post",
        ("GET", "/api/posts/nope/comments", None),
        ("list_comments", json!({ "post_id": "nope" })),
        NotFound,
    )
    .await;
    w.both(
        &w.owner,
        "delete someone else's comment",
        ("DELETE", &format!("/api/comments/{rest_cid}"), None),
        ("delete_comment", json!({ "comment_id": rest_cid })),
        Forbidden,
    )
    .await;
    w.rest(&w.stranger, "DELETE", &format!("/api/comments/{rest_cid}"), None).await;
    w.mcp(&w.stranger, "delete_comment", json!({ "comment_id": mcp_cid })).await;
    w.both(
        &w.stranger,
        "delete unknown comment",
        ("DELETE", "/api/comments/nope", None),
        ("delete_comment", json!({ "comment_id": "nope" })),
        NotFound,
    )
    .await;

    // Changelog: the planned post isn't shipped yet; marking it done is.
    w.both(
        &w.owner,
        "mark done",
        ("PATCH", &format!("/api/posts/{rest_id}"), Some(json!({ "status": "done" }))),
        ("update_post_status", json!({ "id": rest_id, "status": "done" })),
        Ok,
    )
    .await;
    let (r, m) = w
        .both(
            &w.owner,
            "changelog",
            ("GET", "/api/projects/test-app/changelog", None),
            ("get_changelog", json!({ "slug": "test-app" })),
            Ok,
        )
        .await;
    assert_eq!(ids(&r.body["data"]), ids(&m.body["data"]));
    assert_eq!(ids(&r.body["data"]), vec![rest_id.as_str()]);
    w.both(
        &w.owner,
        "changelog since typo",
        ("GET", "/api/projects/test-app/changelog?since=yesterday", None),
        ("get_changelog", json!({ "slug": "test-app", "since": "yesterday" })),
        Invalid,
    )
    .await;

    // Attachments: listed the same way; only the uploader or an admin may delete.
    for name in ["a.png", "b.png"] {
        w.ops
            .store()
            .create_attachment(&rest_id, name, "image/png", 1, &format!("parity-{name}"), &w.owner.actor.id)
            .await
            .unwrap();
    }
    let (r, m) = w
        .both(
            &w.owner,
            "list attachments",
            ("GET", &format!("/api/posts/{rest_id}/attachments"), None),
            ("list_attachments", json!({ "post_id": rest_id })),
            Ok,
        )
        .await;
    let (rest_atts, mcp_atts) =
        (r.body["data"].as_array().unwrap().clone(), m.body["attachments"].as_array().unwrap().clone());
    assert_eq!((rest_atts.len(), mcp_atts.len()), (2, 2));
    let (first, second) =
        (rest_atts[0]["id"].as_str().unwrap().to_string(), rest_atts[1]["id"].as_str().unwrap().to_string());
    w.both(
        &w.stranger,
        "delete someone else's attachment",
        ("DELETE", &format!("/api/attachments/{first}"), None),
        ("delete_attachment", json!({ "id": first })),
        Forbidden,
    )
    .await;
    assert_eq!(w.rest(&w.owner, "DELETE", &format!("/api/attachments/{first}"), None).await.outcome, Ok);
    assert_eq!(w.mcp(&w.owner, "delete_attachment", json!({ "id": second })).await.outcome, Ok);
    let (r, m) = w
        .both(
            &w.owner,
            "attachments after deleting",
            ("GET", &format!("/api/posts/{rest_id}/attachments"), None),
            ("list_attachments", json!({ "post_id": rest_id })),
            Ok,
        )
        .await;
    assert_eq!((r.body["data"].as_array().unwrap().len(), m.body["attachments"].as_array().unwrap().len()), (0, 0));

    // Deleting posts and attachments.
    w.both(
        &w.stranger,
        "delete someone else's post",
        ("DELETE", &format!("/api/posts/{rest_id}"), None),
        ("delete_post", json!({ "id": rest_id })),
        Forbidden,
    )
    .await;
    w.rest(&w.owner, "DELETE", &format!("/api/posts/{rest_id}"), None).await;
    w.mcp(&w.owner, "delete_post", json!({ "id": mcp_id })).await;
    w.both(
        &w.owner,
        "delete post twice",
        ("DELETE", &format!("/api/posts/{rest_id}"), None),
        ("delete_post", json!({ "id": rest_id })),
        NotFound,
    )
    .await;
    w.both(
        &w.owner,
        "unknown attachment",
        ("DELETE", "/api/attachments/nope", None),
        ("delete_attachment", json!({ "id": "nope" })),
        NotFound,
    )
    .await;
    w.both(
        &w.owner,
        "attachments of unknown post",
        ("GET", "/api/posts/nope/attachments", None),
        ("list_attachments", json!({ "post_id": "nope" })),
        NotFound,
    )
    .await;

    // An admin may change and delete a post they didn't write.
    let a = w
        .rest(&w.owner, "POST", "/api/projects/test-app/posts", Some(json!({ "title": "Admin will moderate this" })))
        .await;
    let b = w.rest(&w.owner, "POST", "/api/projects/test-app/posts", Some(json!({ "title": "And this" }))).await;
    let (theirs, also_theirs) = (id_of(&a.body), id_of(&b.body));
    w.both(
        &w.admin,
        "admin changes status",
        ("PATCH", &format!("/api/posts/{theirs}"), Some(json!({ "status": "declined" }))),
        ("update_post_status", json!({ "id": theirs, "status": "declined" })),
        Ok,
    )
    .await;
    w.both(
        &w.admin,
        "admin changes category",
        ("PATCH", &format!("/api/posts/{theirs}"), Some(json!({ "category": "question" }))),
        ("update_post_category", json!({ "id": theirs, "category": "question" })),
        Ok,
    )
    .await;
    assert_eq!(w.rest(&w.admin, "DELETE", &format!("/api/posts/{theirs}"), None).await.outcome, Ok);
    assert_eq!(w.mcp(&w.admin, "delete_post", json!({ "id": also_theirs })).await.outcome, Ok);
}

#[tokio::test]
async fn admin_capabilities_refuse_members_over_rest_and_mcp() {
    scenario_admin(&World::new().await).await;
}

async fn scenario_admin(w: &World) {
    use Outcome::*;

    // Admin-only reads and writes: a member is refused on both transports.
    // (MCP's list_webhooks / get_stats / analytics used to read the store
    // without looking at the actor.)
    let slug = json!({ "slug": "test-app" });
    let cases: Vec<(&str, &str, Option<Value>, &str, Value)> = vec![
        ("list webhooks", "/api/projects/test-app/webhooks", None, "list_webhooks", slug.clone()),
        ("stats", "/api/admin/projects/test-app/stats", None, "get_stats", slug.clone()),
        (
            "analytics",
            "/api/admin/analytics/test-app?days=7",
            None,
            "get_analytics",
            json!({ "slug": "test-app", "days": 7 }),
        ),
        ("top posts", "/api/admin/analytics/test-app/top", None, "get_top_posts", slug.clone()),
    ];
    for (step, uri, _, tool, params) in &cases {
        w.both(&w.owner, step, ("GET", uri, None), (tool, params.clone()), Forbidden).await;
        w.both(&w.admin, step, ("GET", uri, None), (tool, params.clone()), Ok).await;
    }

    // Webhooks: validated, admin-only, scoped to the project.
    let hook = |url: &str| json!({ "url": url });
    w.both(
        &w.owner,
        "member creating a webhook",
        ("POST", "/api/projects/test-app/webhooks", Some(hook("https://example.com/h"))),
        ("create_webhook", json!({ "slug": "test-app", "url": "https://example.com/h" })),
        Forbidden,
    )
    .await;
    w.both(
        &w.admin,
        "insecure url",
        ("POST", "/api/projects/test-app/webhooks", Some(hook("http://example.com/h"))),
        ("create_webhook", json!({ "slug": "test-app", "url": "http://example.com/h" })),
        Invalid,
    )
    .await;
    w.both(
        &w.admin,
        "unknown project",
        ("POST", "/api/projects/nope/webhooks", Some(hook("https://example.com/h"))),
        ("create_webhook", json!({ "slug": "nope", "url": "https://example.com/h" })),
        NotFound,
    )
    .await;
    let rest_hook =
        w.rest(&w.admin, "POST", "/api/projects/test-app/webhooks", Some(hook("https://example.com/r"))).await;
    let mcp_hook =
        w.mcp(&w.admin, "create_webhook", json!({ "slug": "test-app", "url": "https://example.com/m" })).await;
    assert_eq!((rest_hook.outcome, mcp_hook.outcome), (Ok, Ok));
    let (rest_hid, mcp_hid) = (id_of(&rest_hook.body), id_of(&mcp_hook.body));
    let (r, m) = w
        .both(
            &w.admin,
            "list webhooks",
            ("GET", "/api/projects/test-app/webhooks", None),
            ("list_webhooks", slug.clone()),
            Ok,
        )
        .await;
    assert_eq!(r.body["data"].as_array().unwrap().len(), m.body["data"].as_array().unwrap().len());

    w.both(
        &w.owner,
        "member deleting a webhook",
        ("DELETE", &format!("/api/projects/test-app/webhooks/{rest_hid}"), None),
        ("delete_webhook", json!({ "id": rest_hid, "slug": "test-app" })),
        Forbidden,
    )
    .await;
    w.both(
        &w.admin,
        "unknown webhook",
        ("DELETE", "/api/projects/test-app/webhooks/nope", None),
        ("delete_webhook", json!({ "id": "nope", "slug": "test-app" })),
        NotFound,
    )
    .await;
    w.rest(&w.admin, "DELETE", &format!("/api/projects/test-app/webhooks/{rest_hid}"), None).await;
    w.mcp(&w.admin, "delete_webhook", json!({ "id": mcp_hid, "slug": "test-app" })).await;
}

/// Every tool in the catalog is run through a scenario, so a new tool can't
/// skip the REST/MCP comparison.
#[tokio::test]
async fn every_tool_is_exercised_by_a_parity_scenario() {
    let mut called = BTreeSet::new();
    for scenario in 0..3 {
        let w = World::new().await;
        match scenario {
            0 => scenario_projects(&w).await,
            1 => scenario_posts(&w).await,
            _ => scenario_admin(&w).await,
        }
        called.extend(w.called_tools());
    }
    let untested: Vec<&str> = TOOLS.iter().map(|t| t.name).filter(|name| !called.contains(*name)).collect();
    assert!(
        untested.is_empty(),
        "no parity scenario calls these tools (add a step to a scenario in parity_test.rs): {untested:?}"
    );
}
