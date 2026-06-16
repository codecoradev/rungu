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

## v0.2.0 Polish

- [ ] MCP server — implement 12 tools with real data ([#28](https://github.com/codecoradev/rungu/issues/28))
- [ ] Robustness fixes from cora scan ([#33](https://github.com/codecoradev/rungu/issues/33))
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
