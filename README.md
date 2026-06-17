# Rungu

> **Rungu — Listen. Prioritize. Build.**

Lightweight, self-hosted feedback board. Collect feature requests, bug reports, and suggestions from your users — with voting, commenting, and prioritization. Built with Rust + SvelteKit.

## Features

- **Multi-provider auth** — Google, GitHub, Keycloak (ENV-driven, mix-and-match)
- **Email-based identity** — same email across providers = one user
- **Vote & comment** — upvote the features you want
- **Categories & status** — feedback, bug, feature, question → open, planned, in progress, done
- **MCP server** — AI agent tools for Claude Code, Cursor, Windsurf (experimental — tools are stubs, full implementation planned post-MVP)
- **REST API** — full CRUD for posts, votes, comments
- **Single binary** — embedded SPA, Docker ready
- **SQLite** — zero external database dependency

## Quick Start

```bash
# Docker (recommended)
docker compose up -d
# → Frontend: http://localhost:3000
# → Swagger:  http://localhost:3000/swagger-ui

# Or build from source
cargo build --release
./target/release/rungu --db rungu.db serve --listen 0.0.0.0:3000

# Create your first project
./target/release/rungu --db rungu.db project-add "My App" --slug myapp
```

### Local Development (HTTP without HTTPS)

```bash
# .env
RUNGU_SECURE_COOKIE=false    # allow login over plain HTTP
APP_URL=http://localhost:3000
```

## Configuration

```bash
# Copy .env.example to .env
cp .env.example .env

# Configure auth providers (leave empty to disable)
GOOGLE_CLIENT_ID=your-google-client-id
GOOGLE_CLIENT_SECRET=your-google-secret
GITHUB_CLIENT_ID=your-github-client-id
GITHUB_CLIENT_SECRET=your-github-secret
KEYCLOAK_URL=https://auth.example.com
KEYCLOAK_REALM=codecoradev
KEYCLOAK_CLIENT_ID=rungu
KEYCLOAK_CLIENT_SECRET=your-keycloak-secret
```

## Architecture

| Crate | Purpose |
|-------|---------|
| `rungu-proto` | Wire types (Post, Vote, Comment, User, Project) |
| `rungu-core` | Storage, business logic, SQLite queries |
| `rungu-auth` | Multi-provider OAuth + JWT session |
| `rungu-api` | REST API routes (Axum) |
| `rungu-mcp` | MCP server (stdio JSON-RPC 2.0) |
| `rungud` | Daemon binary (CLI: `rungu`) |

## CLI

```bash
rungu serve --db rungu.db --listen 0.0.0.0:3000
rungu project-add "My App" --slug myapp
rungu project-list
rungu healthcheck
rungu mcp
```

See [CLI Reference](https://rungu.pages.dev/cli-reference) for full options.

## MCP Tools

> ⚠️ **Experimental** — tool handlers return stub data. Full implementation tracked in [#28](https://github.com/codecoradev/rungu/issues/28).

12 tools available via stdio for AI coding agents:
- `list_projects`, `get_project`
- `list_posts`, `get_post`, `create_post`, `update_post_status`
- `vote_post`, `search_posts`
- `list_comments`, `add_comment`
- `get_stats`, `get_trending`

## License

Apache-2.0
