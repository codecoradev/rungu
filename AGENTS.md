# AGENTS.md — Rungu Agent Guide

> Panduan untuk AI agents (Claude Code, Cursor, dsb.) yang bekerja di repo ini.
> Pelengkap dari `CLAUDE.md` (konvensi umum). File ini fokus pada **code quality patterns**.

## Clean Code & DRY Conventions

### 1. Auth Guard — gunakan helper, jangan copy-paste

**Owner/admin check** — SUDAH ada helper, JANGAN tulis manual:

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

### 2. Response Envelope — konsisten

Semua handler pakai format yang sama:

```rust
// Success
Ok(Json(serde_json::json!({ "data": value })))

// Created
Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": value }))))

// Error (via ApiError)
Err(ApiError::not_found("Post not found"))
// → auto-renders as { "error": "Post not found" } with correct status code
```

Jangan buat format response baru. Jika butuh pagination:

```rust
Json(serde_json::json!({
    "data": items,
    "pagination": { "page": page, "per_page": per_page, "total": total }
}))
```

---

### 3. Route Module Pattern — wajib ikut struktur

Setiap resource route **harus** expose `pub fn router() -> Router<AppState>`:

```
crates/rungu-api/src/
├── lib.rs            # api_routes() — merge point
├── error.rs          # ApiError + auth helpers
├── post_routes.rs    # pub fn router() -> Router<AppState>
├── vote_routes.rs    # pub fn router() -> Router<AppState>
├── comment_routes.rs # pub fn router() -> Router<AppState>
└── project_routes.rs # pub fn router() -> Router<AppState>
```

**Menambah resource baru** (misal `tag_routes`):

1. Buat `crates/rungu-api/src/tag_routes.rs` dengan `pub fn router()`
2. Register di `lib.rs`: `pub mod tag_routes;` + `.merge(tag_routes::router())`
3. Gunakan `crate::error::ApiError` untuk semua errors

Tidak boleh `.route()` langsung di `server.rs` atau `api_routes()`.

---

### 4. Timestamp Helpers — jangan `.unwrap()` pada `.parse()`

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

Sort/order adalah satu-satunya exception (hardcoded match, bukan user input):

```rust
// ✅ OK — hardcoded match, not interpolation
let order = match params.sort {
    PostSort::Newest => "created_at DESC",
    // ...
};
```

---

### 6. Error Handling — satu tipe, tidak scatter

Semua API errors melalui `ApiError`:

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

Tidak boleh `StatusCode` langsung di handler. Tidak boleh `anyhow::Error` langsung ke response.

---

### 7. Testing Requirements

| Layer | Test type | Location | What to cover |
|-------|-----------|----------|---------------|
| Store | Integration (in-memory SQLite) | `crates/rungu-core/tests/store_test.rs` | CRUD, filters, cascade, edge cases |
| API | Integration (tower::oneshot) | `crates/rungu-api/tests/api_test.rs` | Auth guards (401/403), CRUD, validation |
| Unit | `#[cfg(test)]` in source | alongside handler | Parsing, helpers, pure logic |

**Pattern untuk test setup:**
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

**Minimum coverage sebelum merge:**
- Setiap endpoint publik: test happy path + 404
- Setiap endpoint auth-required: test 401 tanpa token
- Setiap endpoint admin-only: test 403 untuk non-admin
- Store methods: test CRUD lifecycle + edge cases

---

### 8. OpenAPI Documentation

Semua handler **harus** punya `#[utoipa::path(...)]` attribute:

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

Wire types di proto **harus** derive `ToSchema`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Post { ... }
```

Swagger UI: `http://localhost:3000/swagger-ui`

---

## Workflow Konventions

### Branch naming
- Feature: `feat/xxx-api`
- Bugfix: `fix/xxx-description`
- Refactor: `refactor/xxx`
- Branch selalu dari `develop` yang ter-sync

### Commit format
Conventional commits:
- `feat:` new feature
- `fix:` bug fix
- `refactor:` code cleanup (no behavior change)
- `test:` test additions
- `docs:` documentation
- `chore:` tooling, deps

### PR checklist
Sebelum push, PASTIKAN:
```bash
cargo fmt --all -- --check      # formatting
cargo clippy --workspace --all-targets -- -D warnings  # lint
cargo test --workspace           # tests
```

### CI checks (wajib semua hijau kecuali Cora)
- Check, Format, Clippy (-D warnings), Test, Build (release)
- Cargo Audit, Trivy, npm Audit
- Cora Review (bug: exit code 2 flaky — bypass via admin enforcement jika perlu)

### Subagent parallel work
- Tiap subagent kerja di git worktree terisolasi
- File shared yang sering conflict: `lib.rs` (mod registration) + `server.rs` (route merge)
- Setelah merge satu PR, rebased branch lain sebelum merge
