# Roadmap

## v0.1.0 MVP ✅ Released

- [x] Project scaffolding — 6 crates, SQLite schema
- [x] OAuth callback handlers (Google, GitHub, Keycloak)
- [x] JWT session middleware + CurrentUser extractor
- [x] REST API — Posts, Votes, Comments, Projects CRUD (12 endpoints)
- [x] SvelteKit 5 frontend — Board, Post detail, Auth, Admin (5 pages)
- [x] shadcn-svelte UI (button, card, badge, input, textarea)
- [x] Docker — 3-stage build (frontend → Rust → scratch), multi-arch
- [x] CI/CD — 11 checks (check, fmt, clippy, test, build, audit, trivy, cora, docker)
- [x] OpenAPI/Swagger UI at `/swagger-ui`
- [x] VitePress documentation site
- [x] Security hardening (SQL injection fix, CORS, cookie, auth guards)
- [x] Tests — 67 total (47 Rust + 20 TypeScript)

## v0.1.1 Patch ✅ Released

- [x] English tagline ("Listen. Prioritize. Build.")
- [x] Cloudflare Pages project rename
- [x] AGENTS.md translated to English

## v0.1.2 Security patch ✅ Released

Critical/high-severity findings from `cora scan` v0.6.0 (2026-06-17). Patch bump from v0.1.1.

- [x] 🔴 [#54](https://github.com/codecoradev/rungu/issues/54) — MCP server documented as unauthenticated with direct DB access (auth bypass)
- [x] 🔴 [#55](https://github.com/codecoradev/rungu/issues/55) — Email-only account linking enables cross-provider account takeover
- [x] 🟠 [#57](https://github.com/codecoradev/rungu/issues/57) — OAuth flow: redirect lost, provider errors leaked, HTTP client rebuilt per request, weak cookie parse
- [x] [#53](https://github.com/codecoradev/rungu/issues/53) — Docs: correct misleading "first user becomes admin" (actual: `ADMIN_EMAILS` allowlist)

Plus the four grouped scan issues [#56](https://github.com/codecoradev/rungu/issues/56), [#58](https://github.com/codecoradev/rungu/issues/58), [#59](https://github.com/codecoradev/rungu/issues/59), [#60](https://github.com/codecoradev/rungu/issues/60) and the docs-sync follow-up [#70](https://github.com/codecoradev/rungu/issues/70).

Validation artifact: `.cora/scan-validation-2026-06-17.md`

## v0.2.0 Polish

Feature polish for the feedback board. Issues are grouped by area; click through for full scope.

### ✅ Already shipped (kept as history)

- [x] MCP server — 12 tools with real data ([#28](https://github.com/codecoradev/rungu/issues/28))
- [x] Dark mode toggle ([#42](https://github.com/codecoradev/rungu/issues/42))
- [x] Scan-driven cleanup ([#56](https://github.com/codecoradev/rungu/issues/56), [#58](https://github.com/codecoradev/rungu/issues/58), [#59](https://github.com/codecoradev/rungu/issues/59), [#60](https://github.com/codecoradev/rungu/issues/60))

### 🔎 Search & discovery

- [x] [#72](https://github.com/codecoradev/rungu/issues/72) — Complete FTS5 search (index + triggers exist, query path missing)
- [x] [#75](https://github.com/codecoradev/rungu/issues/75) — Public roadmap view (status board)
- [x] [#77](https://github.com/codecoradev/rungu/issues/77) — Auto-generated changelog from done posts

### 💬 Engagement

- [ ] [#73](https://github.com/codecoradev/rungu/issues/73) — Email notifications (new comments, status changes) — **largest item, consider splitting across PRs**
- [x] [#74](https://github.com/codecoradev/rungu/issues/74) — Image attachments on posts
- [x] [#76](https://github.com/codecoradev/rungu/issues/76) — Keyboard shortcuts (board navigation, help overlay) — **good standalone starter**

### Suggested PR order

1. #76 (keyboard shortcuts) — smallest, builds the action/help infrastructure other features can reuse.
2. #72 (FTS5 search) — completes an already-advertised feature; high user value per LOC.
3. #75 (roadmap view) + #77 (changelog) — share UI patterns, land together.
4. #74 (attachments) — schema + storage work, self-contained.
5. #73 (email notifications) — largest, multiple PRs; do last.

## v0.3.0 Production

- [ ] PostgreSQL support (feature flag)
- [ ] Rate limiting
- [ ] Bulk import/export (CSV, JSON)
- [ ] Admin dashboard (analytics, user management)
- [ ] Webhooks (post created, status changed, comment added)
- [ ] Embed widget (for embedding feedback on your own site)
- [ ] Multi-language support (i18n)

## v1.0.0 GA

- [ ] Stable public API (versioned)
- [ ] Performance benchmarks
- [ ] Comprehensive E2E test suite
- [ ] Production deployment guide
- [ ] Cloud offering (managed Rungu)
