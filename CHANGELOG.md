# Changelog

## [0.1.2] - 2026-06-17

Security and correctness patch. Addresses the critical/high findings from the
2026-06-17 `cora scan` (profile `rust-strict`) that were validated against
source — 41 valid findings grouped into 8 issues (#53–#60), all resolved.

This is a **patch release**: no breaking API changes, no schema migrations
required, and the Docker entrypoint is unchanged. Operators should still review
the Security section below.

### Security

- **🔴 Verified-email gate on OAuth account linking (#55, #64).** Accounts
  are now linked by email **only** when the provider asserts `email_verified:
  true`. Google/Keycloak read the standard userinfo claim; GitHub's
  verification is determined from `/user/emails` (primary + verified).
  Unverified emails are rejected with `HTTP 403` before any DB write. Prevents
  cross-provider account takeover via an attacker-controlled or misconfigured
  IdP (e.g. self-hosted Keycloak realm with admin-reassignable emails).
- **🔴 MCP server trust boundary documented (#54, #62).** The MCP server is
  intentionally unauthenticated — that posture is now documented loudly
  (critical-warning banner + dedicated *Trust Boundary & Security* section in
  `docs/integrations/mcp.md` and a security-model doc-comment on the crate
  root). No behavior change; the doc previously normalized unsafe deployments.
- **Strict `RUNGU_SECURE_COOKIE` parsing (#57, #65).** The env var now uses a
  fail-fast boolean parser (`true|1|yes|on`, `false|0|no|off`, case-insensitive).
  Typos like `False`, `0`, or `no` no longer silently enable secure cookies
  and break local HTTP login; instead the process exits at startup with an
  actionable error.
- **OAuth redirect target preserved across the round-trip (#57, #65).** The
  requested post-login destination is now carried in an HMAC-signed
  `oauth_redirect` cookie (5-min expiry, verified server-side) instead of a
  dead `redirect_target` query param that providers never echoed. The
  callback no longer hardcodes `/`.
- **Generic OAuth error responses (#57, #65).** Provider-reported errors are
  logged internally but the client-facing body is now the generic
  `"OAuth login failed"`, preventing reflection of untrusted provider strings.
- **API docs hardened (#60, #69).** `APP_SECRET=your-secret-here` placeholder
  replaced with `openssl rand -hex 32` guidance in quick-start, docker, and
  from-source examples. Docker Compose default `APP_URL=http://localhost:3000`
  is now explicitly labeled local-dev-only.

### Fixed

- **`list_posts` no longer silently drops unknown filters (#56, #66).**
  `?status=opennnn` / `?category=notarealcategory` now return `400 Bad Request`
  instead of `200 OK` with unfiltered results.
- **`list_comments` verifies the parent post exists (#56, #66).** Requests for
  a nonexistent post now return `404` instead of `200 OK` with an empty list.
- **Empty-result pagination reports `total_pages: 1` (#56, #66).** A 1-based
  API no longer returns the internally-inconsistent "page 1 of 0 pages".
- **TS client serializes falsy `page` / `per_page` (#56, #66).**
  `page: 0` is no longer dropped from the query string.
- **`timeAgo()` / `formatDate()` validate dates (#59, #68).** Invalid input
  returns `''` instead of leaking `"NaNy ago"` / locale-dependent
  `"Invalid Date"` text into the UI.
- **Board page `$effect` no longer self-triggers (#58, #67).** The effect
  now depends only on `slug`; duplicate fetches and the fetch-loop risk are
  gone.
- **Admin page checks the fetched user directly (#58, #67).** Reads
  `currentUser.role` instead of a lazily-recomputed `$derived(isAdmin)`, so
  the in-tick check can't deny access to real admins.
- **Deleting a comment prunes its descendants locally (#58, #67).** Replies
  whose `parent_id` chain led to the deleted comment are removed from client
  state (no more invisible-but-counted orphans).
- **Creator display has a `'User'` fallback (#59, #68).**
  `post.creator.name || post.creator.email || 'User'`.

### Changed

- **Shared HTTP client for OAuth calls (#57, #65).** `AppState` now holds a
  single `reqwest::Client`, replacing the per-callback `Client::builder()`.
- **Badge / Button sanitize `href` (#59, #68).** Unsafe schemes
  (`javascript:`, `data:`, protocol-relative `//host`) are dropped via a
  shared `sanitizeHref()` helper; unsafe values fall back to rendering a
  `<span>` / `<button>`.
- **File input no longer binds `value` (#59, #68).** Browser-restricted file
  input value is no longer two-way bound; selection still flows through
  `bind:files`.

### Documentation

- **Auth overview corrected (#53, #63).** The docs no longer claim "first user
  is automatically admin". The actual mechanism — the `ADMIN_EMAILS` env
  allowlist — is now documented, with the empty/unset behavior spelled out.
- **Configuration reference synced with code (#70).** Removed nonexistent
  `*_REDIRECT_URI` env vars; added `ADMIN_EMAILS`, `DATABASE_URL`, `RUST_LOG`;
  expanded `APP_SECRET` / `RUNGU_SECURE_COOKIE` / `RUNGU_CORS_ORIGINS` docs;
  added a **Security** subsection.
- **CLI examples consistent (#60, #69).** `project-add` / `project-list`
  everywhere (was `project add` / `project list` in the README).
  `cargo run -p rungud -- serve` for contributors.
- **API reference path notation standardized (#70).** `:slug` → `{slug}`
  to match Axum's route syntax.
- **Roadmap updated (#61).** Added the `v0.1.2 Security patch` section and
  expanded `v0.2.0 Polish` with concrete issue links.

### Quality

- `cora` pre-commit hook installed (`cora hook install`) — every commit in
  this release ran through `cora review` locally.
- 10 new tests: 6 OAuth-parser unit tests (`rungu-auth` + `rungu-api`),
  4 API validation integration tests, 12 frontend `sanitizeHref` / date tests.
- Quality gate at release time: 0 critical, 0 security, 0 performance findings
  on the net diff (the scan that motivated this patch reported 2 critical / 10
  security findings — all resolved or documented).

### Minimal upgrade notes

- **No migrations.** SQLite schema is unchanged.
- **No breaking API changes.** All endpoints, request shapes, and response
  shapes are backwards-compatible.
- **Operators of self-hosted Keycloak realms:** confirm email verification is
  enabled upstream, otherwise end users will now see `HTTP 403` on login. This
  is the intended behavior of #55.
- **Anyone with `RUNGU_SECURE_COOKIE=False` / `0` / `no` in their env:**
  update to the lowercase form or the process will exit at startup. This is
  the intended behavior of #57.

## [0.1.1] - 2026-06-16

### Fixed
- Translate tagline from Indonesian to English ("Listen. Prioritize. Build.")
- Rename Cloudflare Pages project to `rungu-docs` (rungu.pages.dev was taken)
- Translate AGENTS.md from Indonesian to English

## [0.1.0] - 2026-06-16

First MVP release. Full-stack feedback board with OAuth, REST API, and embedded SPA.

### Auth
- **OAuth providers**: Google, GitHub, Keycloak (ENV-driven, mix-and-match)
- **Email-based identity**: same email across providers = one user
- **JWT sessions**: HS256, 7-day expiry, HttpOnly + Secure cookie
- **`RUNGU_SECURE_COOKIE`**: set `false` for local HTTP development
- **CurrentUser extractor**: Axum `FromRequestParts` with generic state support

### REST API (12 endpoints)
- **Projects**: `GET/POST /api/projects`, `GET/PATCH/DELETE /api/projects/:slug`
- **Posts**: `GET/POST /api/projects/:slug/posts`, `GET/PATCH/DELETE /api/posts/:id`
- **Votes**: `POST /api/posts/:id/vote` (toggle), `GET /api/posts/:id/vote` (check)
- **Comments**: `GET/POST /api/posts/:id/comments`, `DELETE /api/comments/:id`
- **Auth**: `GET /auth/:provider/login`, `/callback`, `POST /auth/logout`, `GET /auth/me`

### Frontend (SvelteKit 5 + shadcn-svelte)
- **5 pages**: landing, board, post detail, login, admin
- **7 components**: PostCard, VoteButton, CommentThread, PostForm, AuthProviderButtons, StatusBadge, CategoryBadge
- **shadcn-svelte UI**: button, badge, card, input, textarea, label, separator, skeleton
- **Tailwind v4** with dark-first oklch theme
- **Typed API client** mirroring all REST endpoints

### Security
- **SQL injection fix**: parameterized queries in `list_posts` (LIKE ? with bind)
- **Auth guards**: `ApiError::check_owner_or_admin()`, `ApiError::require_admin()`
- **Zero `unwrap()`** in production code (CLAUDE.md compliance)
- **CSRF protection**: state cookie for OAuth flow
- **Cookie security**: `RUNGU_SECURE_COOKIE` flag for HTTP dev mode

### Tests (67 total)
- **Store integration** (11): CRUD, filters, cascade deletes, SQL injection, pagination
- **API integration** (12): HTTP lifecycle, auth guards (401/403), validation
- **Unit** (24): parsing, JWT middleware, cookie helpers, OAuth helpers
- **Frontend** (20): utils (cn, timeAgo, formatDate), API client (all endpoints)

### Documentation
- **OpenAPI/Swagger UI** at `/swagger-ui` (15 documented endpoints)
- **OpenAPI JSON spec** at `/api-docs/openapi.json`
- **AGENTS.md** with clean code & DRY conventions
- **Pre-commit hook**: cora review + cargo fmt + clippy

### Infrastructure
- **Docker**: 3-stage build (frontend → Rust → scratch), ~15MB image
- **docker-compose**: volume, healthcheck, all OAuth ENV vars
- **CI**: 11 checks (check, fmt, clippy, test, build, cargo audit, trivy, npm audit, cora review, docker build)
- **Release workflow**: multi-platform binaries (amd64, arm64, macOS, Windows) + Docker GHCR push

## [0.0.1] - 2026-06-15

### Added
- Project scaffolding — workspace with 6 crates
- `rungu-proto` — wire types (Post, Vote, Comment, User, Project)
- `rungu-core` — SQLite storage layer with full CRUD
- `rungu-auth` — multi-provider OAuth config (Google/GitHub/Keycloak) + JWT session
- `rungu-api` — REST API route stubs
- `rungu-mcp` — MCP server with 12 tool stubs (stdio JSON-RPC 2.0)
- `rungud` — daemon binary with CLI subcommands (serve, project add/list, healthcheck, mcp)
- Initial migration (001_initial.sql) — users, user_identities, projects, posts, votes, comments
- Dockerfile (multi-stage: frontend → Rust builder → scratch)
- docker-compose.yml
- SvelteKit 5 frontend scaffold
