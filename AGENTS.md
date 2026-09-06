# AGENTS.md — Rungu Agent Guide

> Guide for AI agents (Claude Code, Cursor, etc.) working in this repo.
> Complements `CLAUDE.md` (general conventions). This file focuses on **code quality patterns**.

## Clean Code & DRY Conventions

### 1. Auth Guard — use helpers, do not copy-paste

**Owner/admin check** — helper already exists, DO NOT write manually:

```rust
// ✅ DO THIS
ApiError::check_owner_or_admin(&user, &resource.created_by, "You can only modify your own posts")?;

// ❌ NOT THIS (was duplicated 3x before refactor)
let is_author = resource.created_by == user.id;
let is_admin = user.role == UserRole::Admin;
if !is_author && !is_admin {
    return Err(ApiError::forbidden("..."));
}
```

**Admin-only check**:

```rust
// ✅ DO THIS
ApiError::require_admin(&user)?;

// ❌ NOT THIS
if user.role != UserRole::Admin {
    return Err(ApiError::forbidden("Admin access required"));
}
```

Helper location: `crates/rungu-api/src/error.rs`

---

### 2. Response Envelope — be consistent

All handlers use the same format:

```rust
// Success
Ok(Json(serde_json::json!({ "data": value })))

// Created
Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": value }))))

// Error (via ApiError)
Err(ApiError::not_found("Post not found"))
// → auto-renders as { "error": "Post not found" } with correct status code
```

Do not create new response formats. If you need pagination:

```rust
Json(serde_json::json!({
    "data": items,
    "pagination": { "page": page, "per_page": per_page, "total": total }
}))
```

---

### 3. Route Module Pattern — must follow the structure

Each resource route **must** expose `pub fn router() -> Router<AppState>`:

```
crates/rungu-api/src/
├── lib.rs            # api_routes() — merge point
├── error.rs          # ApiError + auth helpers
├── post_routes.rs    # pub fn router() -> Router<AppState>
├── vote_routes.rs    # pub fn router() -> Router<AppState>
├── comment_routes.rs # pub fn router() -> Router<AppState>
└── project_routes.rs # pub fn router() -> Router<AppState>
```

**Adding a new resource** (e.g. `tag_routes`):

1. Create `crates/rungu-api/src/tag_routes.rs` with `pub fn router()`
2. Register in `lib.rs`: `pub mod tag_routes;` + `.merge(tag_routes::router())`
3. Use `crate::error::ApiError` for all errors

Do not place `.route()` calls directly in `server.rs` or `api_routes()`.

---

### 4. Timestamp Helpers — do not `.unwrap()` on `.parse()`

```rust
// ✅ DO THIS — parse_now for self-generated timestamps
let now = Utc::now().to_rfc3339();
created_at: parse_now(&now),

// ✅ DO THIS — parse_ts for DB-read timestamps (has fallback)
let created_at = parse_ts(row.get("created_at"));

// ❌ NOT THIS — unwrap() on parse
created_at: now.parse().unwrap(),  // BANNED in production code
```

Helper location: `crates/rungu-core/src/store.rs`

---

### 5. SQL Queries — parameterized, no string interpolation

```rust
// ✅ DO THIS — positional ? binding
conditions.push("(title LIKE ? OR description LIKE ?)");
let pattern = format!("%{q}%");
query.bind(pattern.clone()).bind(pattern);

// ❌ NOT THIS — SQL INJECTION
format!("(title LIKE '%{q}%')");  // NEVER DO THIS
```

Sort/order is the only exception (hardcoded match, not user input):

```rust
// ✅ OK — hardcoded match, not interpolation
let order = match params.sort {
    PostSort::Newest => "created_at DESC",
    // ...
};
```

---

### 6. Error Handling — one type, no scatter

All API errors go through `ApiError`:

```rust
// ✅ Handler signature
async fn handler() -> Result<impl IntoResponse, ApiError> { ... }

// ✅ Error construction helpers
ApiError::bad_request("Title is required")
ApiError::not_found("Post not found")
ApiError::forbidden("Admin access required")
ApiError::internal("Unexpected error")

// ✅ From<anyhow::Error> auto-converts store errors to 500
state.store.create_post(...).await?;  // ? → ApiError::internal
```

Do not use `StatusCode` directly in handlers. Do not leak `anyhow::Error` to responses.

---

### 7. Testing Requirements

| Layer | Test type | Location | What to cover |
|-------|-----------|----------|---------------|
| Store | Integration (in-memory SQLite) | `crates/rungu-core/tests/store_test.rs` | CRUD, filters, cascade, edge cases |
| API | Integration (tower::oneshot) | `crates/rungu-api/tests/api_test.rs` | Auth guards (401/403), CRUD, validation |
| Unit | `#[cfg(test)]` in source | alongside handler | Parsing, helpers, pure logic |

**Test setup pattern:**
```rust
// Store test
async fn setup() -> Store {
    let pool = open_pool(":memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    Store::new(pool)
}

// API test
let (app, store) = setup_app().await;
let response = app.oneshot(Request::builder().uri("/projects").body(Body::empty()).unwrap()).await.unwrap();
```

**Minimum coverage before merge:**
- Every public endpoint: test happy path + 404
- Every auth-required endpoint: test 401 without token
- Every admin-only endpoint: test 403 for non-admin
- Store methods: test CRUD lifecycle + edge cases

---

### 8. OpenAPI Documentation

All handlers **must** have `#[utoipa::path(...)]` attribute:

```rust
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/posts",
    params(("slug" = String, Path, description = "Project slug")),
    responses(
        (status = 200, description = "List of posts", body = serde_json::Value),
        (status = 404, description = "Project not found"),
    ),
    tag = "posts",
)]
async fn list_posts(...) -> Result<impl IntoResponse, ApiError> { ... }
```

Wire types in proto **must** derive `ToSchema`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Post { ... }
```

Swagger UI: `http://localhost:3000/swagger-ui`

---

## Workflow Conventions

### Branch naming
- Feature: `feat/xxx-api`
- Bugfix: `fix/xxx-description`
- Refactor: `refactor/xxx`
- Always branch from up-to-date `develop`

### Commit format
Conventional commits:
- `feat:` new feature
- `fix:` bug fix
- `refactor:` code cleanup (no behavior change)
- `test:` test additions
- `docs:` documentation
- `chore:` tooling, deps

### PR checklist
Before pushing, ENSURE:
```bash
cargo fmt --all -- --check      # formatting
cargo clippy --workspace --all-targets -- -D warnings  # lint
cargo test --workspace           # tests
```

### CI checks (all must be green except Cora)
- Check, Format, Clippy (-D warnings), Test, Build (release)
- Cargo Audit, Trivy, npm Audit
- Cora Review (known bug: exit code 2 flaky — bypass via admin enforcement if needed)

### Subagent parallel work
- Each subagent works in an isolated git worktree
- Shared files that frequently conflict: `lib.rs` (module registration) + `server.rs` (route merge)
- After merging one PR, rebase other branches before merging

---

## Analytics (privacy-first, #186)

- Events land in `analytics_events` (migration 003). Capture via
  `crate::analytics::capture(&state.store, &project_id, post_id, event_type)` —
  fire-and-forget (detached task); NEVER let capture failures bubble to the handler.
- Five event types: `board_view`, `post_view`, `vote`, `comment`, `post_created`.
- Privacy invariant: NO IP, cookies, or fingerprint ever stored. Do not add
  request-scoped identifiers to `analytics_events`.
- Aggregates: `analytics_totals`, `analytics_daily`, `analytics_top_posts`
  (dialect-branched: SQLite `datetime('now', …)` vs PostgreSQL
  `NOW() - (? || ' days')::interval`).
- Admin UI: Analytics tab in `/admin` (project selector + 7d/30d/All-time).

## White-label branding + license (#185)

- Branding: `InstanceBranding` in `rungu-api/src/meta.rs`, from
  `RUNGU_INSTANCE_NAME` / `RUNGU_LOGO_URL` / `RUNGU_FOOTER_TEXT`.
- Served via public `GET /api/meta` (camelCase: `#[serde(rename_all = "camelCase")]`)
  and injected into the SPA shell as `window.__RUNGU_META__` by
  `rungud/src/spa.rs` (`inject_branding`). The static SvelteKit shell CANNOT read
  response headers at boot — server-side injection is the mechanism.
- The "Powered by Rungu" badge is the OSS growth loop: hard-coded default, hidden
  ONLY by a valid Polar license (`RUNGU_LICENSE_KEY` + `RUNGU_LICENSE_ORG_ID`,
  constant-checked at startup; 5xx/429 = fail-open, keep last-known-good).
- Adding a field to `AppState`? Update ALL test `AppState` literals
  (`api_test.rs`, `analytics_test.rs`, `branding_test.rs`, `remote_access_test.rs`,
  `webhook_embed_test.rs`) — CI Docker Build will catch a missed one.
- `sqlx::Any` REJECTS `foreign_keys=on` as a URL parameter. Enforce per-connection
  SQLite PRAGMAs via `PoolOptions::after_connect` in `rungu-core::open_pool`
  (keep the in-memory `max_connections(1)` special case: each connection is a
  separate empty DB).

## Remote machine access (#189)

- `RUNGU_API_KEY` (env) + `Authorization: Bearer <key>` = the synthetic
  `ai-agent` admin (`agent@rungu.local`, bootstrapped in `serve()`; its resolved
  DB id reaches extractors via `AppState.agent_user_id` → `AgentUserId`).
- `POST /mcp` = MCP over HTTP (JSON-RPC in/out) — reuses
  `rungu_mcp::handle_message`; auth via `rungu_auth::middleware::verify_api_key`.
- Wrong key on privileged endpoints = always 401, never falls through to cookies.
- MCP conventions: notifications (no `id`) emit no response; unknown methods are
  -32601; new tools MUST return the `{data: ...}` envelope, keep
  `parse_category_strict`-style input validation, and update README/docs tool
  count (26 as of v0.4.0).
