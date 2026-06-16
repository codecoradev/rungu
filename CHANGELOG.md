# Changelog

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
