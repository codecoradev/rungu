# Rungu

> **Rungu — Listen. Prioritize. Build.**

- MCP Registry name: `mcp-name: io.github.codecoradev/rungu`

Lightweight, self-hosted feedback board. Collect feature requests, bug reports, and suggestions from your users — with voting, commenting, and prioritization. Built with Rust + SvelteKit.

## Features

- **Multi-provider auth** — Google, GitHub, Keycloak and any OpenID Connect issuer such as Gatehouse (ENV-driven, mix-and-match, PKCE)
- **Email-based identity** — same email across providers = one user
- **Vote & comment** — upvote the features you want
- **Image attachments** — screenshots and mockups on posts (PNG/JPEG/WebP/GIF, max 10MB)
- **Categories & status** — feedback, bug, feature, question → open, planned, in progress, done
- **Full-text search** — FTS5-powered search across all posts
- **Dark mode** — light/dark/system theme toggle
- **Keyboard shortcuts** — full board navigation without a mouse
- **MCP server** — 26 AI agent tools for Claude Code, Cursor, Windsurf, over stdio or HTTP (API key, or OAuth for Claude connectors)
- **REST API** — full CRUD for posts, votes, comments, attachments, with OpenAPI docs at `/swagger-ui`
- **Dual database** — SQLite (default) or PostgreSQL via `DATABASE_URL`
- **Single binary** — embedded SPA, Docker ready

## Screenshots

The screenshots use demo data (a fictional note-taking app, "Acme Notes").

| Boards | Board |
|:---:|:---:|
| ![Boards](docs/screenshots/home-desktop.png) | ![Board](docs/screenshots/board-desktop.png) |

| Roadmap | Changelog |
|:---:|:---:|
| ![Roadmap](docs/screenshots/roadmap-desktop.png) | ![Changelog](docs/screenshots/changelog-desktop.png) |

| New post (similar posts are suggested while typing) | Filters and sorting |
|:---:|:---:|
| ![New post](docs/screenshots/new-post-desktop.png) | ![Filters](docs/screenshots/board-filters-desktop.png) |

| Post detail (attachment, official team response, threaded comments) | Dark mode |
|:---:|:---:|
| ![Post detail](docs/screenshots/post-detail-desktop.png) | ![Dark mode](docs/screenshots/board-desktop-dark.png) |

| Sign in (Google, GitHub, OpenID Connect) | API docs (`/swagger-ui`) |
|:---:|:---:|
| ![Login](docs/screenshots/login-desktop.png) | ![API docs](docs/screenshots/api-docs-desktop.png) |

<details>
<summary>Admin</summary>

| Projects | Moderation |
|:---:|:---:|
| ![Admin projects](docs/screenshots/admin-projects-desktop.png) | ![Admin moderation](docs/screenshots/admin-moderation-desktop.png) |

| Webhooks | Analytics |
|:---:|:---:|
| ![Admin webhooks](docs/screenshots/admin-webhooks-desktop.png) | ![Admin analytics](docs/screenshots/admin-analytics-desktop.png) |

</details>

<details>
<summary>Mobile</summary>

| Boards | Board | Roadmap | Post | Sign in |
|:---:|:---:|:---:|:---:|:---:|
| ![Boards](docs/screenshots/home-mobile.png) | ![Board](docs/screenshots/board-mobile.png) | ![Roadmap](docs/screenshots/roadmap-mobile.png) | ![Post](docs/screenshots/post-detail-mobile.png) | ![Sign in](docs/screenshots/login-mobile.png) |

</details>

<details>
<summary>Dark mode</summary>

| Boards | Post detail |
|:---:|:---:|
| ![Boards](docs/screenshots/home-desktop-dark.png) | ![Post detail](docs/screenshots/post-detail-desktop-dark.png) |

| Roadmap | Changelog |
|:---:|:---:|
| ![Roadmap](docs/screenshots/roadmap-desktop-dark.png) | ![Changelog](docs/screenshots/changelog-desktop-dark.png) |

| Sign in | Admin | Board (mobile) |
|:---:|:---:|:---:|
| ![Sign in](docs/screenshots/login-desktop-dark.png) | ![Admin](docs/screenshots/admin-projects-desktop-dark.png) | ![Board mobile](docs/screenshots/board-mobile-dark.png) |

</details>

<details>
<summary>Errors</summary>

| Post not found |
|:---:|
| ![Post not found](docs/screenshots/post-not-found-desktop.png) |

</details>

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
| `rungu-auth` | Multi-provider OAuth/OIDC + JWT session |
| `rungu-api` | REST API routes (Axum) |
| `rungu-mcp` | MCP server (JSON-RPC 2.0 over stdio and `/mcp`) |
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

26 tools for AI coding agents (stdio locally, HTTP remotely with `RUNGU_API_KEY`):

**Projects:** `list_projects`, `get_project`
**Posts:** `list_posts`, `get_post`, `create_post`, `update_post_status`, `update_post_category`, `delete_post`, `vote_post`, `search_posts`, `get_roadmap`
**Comments:** `list_comments`, `add_comment`, `delete_comment`
**Changelog:** `get_changelog`
**Attachments:** `list_attachments`, `delete_attachment`
**Insights:** `get_stats`, `get_trending`, `get_analytics`, `get_top_posts`
**Admin:** `create_project`, `delete_project`, `list_webhooks`, `create_webhook`, `delete_webhook`

Connect from any MCP-compatible client (Claude Desktop, Cursor, etc.):

Local (stdio):
```json
{
  "mcpServers": {
    "rungu": {
      "command": "rungu",
      "args": ["mcp"],
      "env": {
        "RUNGU_DB_URL": "sqlite://path/to/rungu.db"
      }
    }
  }
}
```

Remote (HTTP, with the `RUNGU_API_KEY` Bearer key):
```json
{
  "mcpServers": {
    "rungu": {
      "url": "https://your-rungu-host/mcp",
      "headers": { "Authorization": "Bearer <RUNGU_API_KEY>" }
    }
  }
}
```

Claude connectors sign in with OAuth instead of a shared key: set `RUNGU_MCP_OAUTH_ISSUER` (for example Gatehouse) and add `https://your-rungu-host/mcp` as the connector URL. See [MCP](docs/integrations/mcp.md#connect-from-claude-oauth).

## License

Apache-2.0
