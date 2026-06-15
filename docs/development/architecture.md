# Architecture

## Overview

Rungu is a Rust backend with an embedded SvelteKit SPA, deployed as a single binary.

```
┌─────────────────────────────────────────┐
│              Single Binary              │
│  ┌─────────────┐  ┌──────────────────┐  │
│  │  Axum HTTP  │  │   SvelteKit SPA  │  │
│  │   Server    │  │  (rust-embed)    │  │
│  └──────┬──────┘  └──────────────────┘  │
│         │                               │
│  ┌──────┴──────┐                        │
│  │   rungu-core│                        │
│  │   (SQLite)  │                        │
│  └─────────────┘                        │
└─────────────────────────────────────────┘
         │
    ┌────┴────┐
    │ rungu.db │
    │ (SQLite) │
    └─────────┘
```

## Crate Layout

```
rungu/
├── rungu-proto/     Wire types — no logic
├── rungu-core/      Storage + business logic (SQLite/sqlx)
├── rungu-auth/      Multi-provider OAuth + JWT session
├── rungu-api/       REST API routes (Axum handlers)
├── rungu-mcp/       MCP server (stdio JSON-RPC 2.0)
└── rungud/          Binary entrypoint (CLI + server)
```

### rungu-proto

Shared types: `Post`, `Vote`, `Comment`, `User`, `Project`, enums, request/response types. No business logic.

### rungu-core

Database layer: connection pool (WAL mode), migrations, all SQLite queries. Depends only on `rungu-proto` and `sqlx`.

### rungu-auth

OAuth providers (Google, GitHub, Keycloak), JWT session issuance and validation, Axum middleware (`CurrentUser` extractor). ENV-driven configuration.

### rungu-api

Axum route handlers. Orchestrates `rungu-core` calls with `rungu-auth` middleware. Maps HTTP requests to store operations.

### rungu-mcp

MCP server over stdio. 12 tools for AI agent integration. No auth (process isolation). Calls `rungu-core` directly.

### rungud

Binary entrypoint. CLI subcommands (`serve`, `project add`, `project list`, `healthcheck`, `mcp`). Embeds the SvelteKit SPA via `rust-embed`.

## Data Flow

### OAuth Login

```
Browser → /auth/google/login → redirect to Google
→ Google consent → /auth/google/callback?code=xxx
→ rungu-auth: exchange code → get identity
→ rungu-core: find_or_create_user(email)
→ rungu-auth: issue_jwt()
→ Set session cookie → redirect to /
```

### Create Post

```
Browser → POST /api/projects/:slug/posts (with session cookie)
→ rungu-api: validate auth via CurrentUser extractor
→ rungu-core: store.create_post()
→ Response: created post JSON
```

### MCP Query

```
AI Agent → rungu mcp (subprocess stdin)
→ rungu-mcp: parse JSON-RPC, route to handler
→ rungu-core: store.list_posts()
→ stdout: JSON-RPC response
```

## Deployment

- **Single binary** — no external dependencies
- **Embedded SPA** — SvelteKit build via `rust-embed`
- **SQLite** — zero-config database, WAL mode for concurrent reads
- **Docker** — multi-stage build: Node (frontend) → Rust (backend) → scratch
