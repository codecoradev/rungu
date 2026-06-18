# Progress

## Status
✅ Completed — Documentation audit (scout-docs)
✅ Completed — API scout audit (scout-api)

## Tasks

### Doc Scout Audit
- [x] Scanned all 20 `docs/**/*.md` files + README, CHANGELOG, .env.example
- [x] Wrote per-file 1-line summaries (23 files total)
- [x] Cross-referenced docs against source code in `crates/` and `web/`
- [x] Flagged 6 issues across 3 severity levels

### API Scout Audit
- [x] Scanned ALL API route files in `crates/rungu-api/src/` (7 files: attachment, auth, comment, post, project, vote routes + error/oauth/lib)
- [x] Scanned MCP server in `crates/rungu-mcp/src/lib.rs` (13 tools)
- [x] Scanned `openapi.rs` (17 of 27 endpoints documented)
- [x] Scanned `store.rs` (full CRUD for all entities including attachments)
- [x] Scanned proto types in `crates/rungu-proto/src/lib.rs`
- [x] Cross-referenced all handlers, MCP tools, OpenAPI entries, and store methods
- [x] Flagged 20 issues across 4 severity levels
- [x] Wrote full report to `/tmp/scout-api.md`

## Files Changed
- `/tmp/scout-docs.md` — Full doc audit report (created)
- `/tmp/scout-api.md` — Full API audit report (created)

## API Audit Findings (20 issues)

### 🔴 High (4)
1. **All 4 attachment endpoints missing from OpenAPI** — Handlers carry `#[utoipa::path]` attrs but are never registered in `paths(...)`. Schemas also missing.
2. **All 5 auth endpoints missing from OpenAPI** — No `#[utoipa::path]` attrs on handlers, not in `paths(...)`.
3. **MCP has no attachment tools** — 13 tools but zero for attachment list/upload/download/delete.
4. **`list_attachments` has no auth and no post-existence check** — Unlike `list_comments` which verifies post exists first.

### 🟠 Medium (8)
5. MCP missing `delete_post` tool (REST has `DELETE /api/posts/{id}`)
6. MCP missing `delete_comment` tool
7. MCP missing project CRUD (create/update/delete) — possibly intentional
8. OpenAPI `body` types don't match actual `{ "data": ... }` envelope (all endpoints)
9. List endpoints inconsistent on pagination (posts/changelog have it, comments/attachments/projects don't)
10. `upload_attachment` returns bare object, all other create endpoints wrap in `{ "data": ... }`
11. Auth errors return plain text, API errors return `{ "error": ... }` JSON
12. MCP `update_post_status` has no ownership check (REST enforces owner/admin)

### 🟡 Low (8)
13. MCP missing `get_roadmap` tool (REST has it)
14. OpenAPI uses `serde_json::Value` body for many endpoints — no concrete schema
15. MCP `get_stats` counts are wrong for projects with >1000 posts
16. MCP response shapes differ from REST (`{ "posts": [...] }` vs `{ "data": [...] }`)
17. Duplicate `OAuthIdentity` in proto (dead, no `email_verified`) vs oauth.rs (live)
18. Duplicate `*Request` vs `*Body` structs in proto (4 pairs, `*Request` unused)
19. Unused proto types: `ListResponse<T>`, `ListPostsQuery`, `ProjectStats`
20. `GET /health` not in OpenAPI (minor)

## Inventory Summary
- **27 REST endpoints** total (21 API + 5 auth + 1 health)
- **17 of 27** documented in OpenAPI (63%)
- **13 MCP tools** (read-heavy: 8 read, 5 write)
- **6 MCP↔REST parity gaps** (attachments entirely, delete_post, delete_comment, roadmap, project CRUD)
