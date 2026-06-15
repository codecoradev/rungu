# Changelog

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
