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

## v0.1.2 Security patch

Critical/high-severity findings from `cora scan` v0.6.0 (2026-06-17). Ships as patch bump from 0.1.1 before v0.2.0 feature work.

- [ ] 🔴 [#54](https://github.com/codecoradev/rungu/issues/54) — MCP server documented as unauthenticated with direct DB access (auth bypass)
- [ ] 🔴 [#55](https://github.com/codecoradev/rungu/issues/55) — Email-only account linking enables cross-provider account takeover
- [ ] 🟠 [#57](https://github.com/codecoradev/rungu/issues/57) — OAuth flow: redirect lost, provider errors leaked, HTTP client rebuilt per request, weak cookie parse
- [ ] [#53](https://github.com/codecoradev/rungu/issues/53) — Docs: correct misleading "first user becomes admin" (actual: `ADMIN_EMAILS` allowlist)

Validation artifact: `.cora/scan-validation-2026-06-17.md`

## v0.2.0 Polish

- [ ] MCP server — implement 12 tools with real data ([#28](https://github.com/codecoradev/rungu/issues/28))
- [ ] [#56](https://github.com/codecoradev/rungu/issues/56) — API: silent filter drops, missing post-existence check, pagination edge cases
- [ ] [#58](https://github.com/codecoradev/rungu/issues/58) — Frontend state: `$effect` duplicate fetches, stale admin check, orphaned comment replies
- [ ] [#59](https://github.com/codecoradev/rungu/issues/59) — Frontend components: unsafe date handling, file-input bind, unvalidated `href`, missing creator fallback
- [ ] [#60](https://github.com/codecoradev/rungu/issues/60) — Docs: CLI command-name inconsistencies, weak `APP_SECRET` placeholder, HTTP-default `APP_URL`, missing secret in Docker run
- [ ] Full-text search with SQLite FTS5
- [ ] Roadmap view (public status board)
- [ ] Changelog auto-generation (from done posts)
- [ ] Email notifications (new comments, status changes)
- [ ] Post attachments (images)
- [ ] Keyboard shortcuts
- [ ] Dark mode toggle

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
