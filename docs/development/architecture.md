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
├── rungu-mcp/       MCP tools (JSON-RPC 2.0; stdio and /mcp over HTTP)
└── rungud/          Binary entrypoint (CLI + server)
```

### rungu-proto

Shared types: `Post`, `Vote`, `Comment`, `User`, `Project`, enums, request/response types. No business logic.

### rungu-core

Database layer: connection pool (WAL mode), migrations, all SQLite queries. Depends only on `rungu-proto` and `sqlx`.

Also home of the domain operations module (`ops::Operations`): every mutating use case (create/update/delete post, vote, comment, official response, attachment delete) lives there once — existence check → authorization → validation → write → storage cleanup → `DomainEvent`. REST and MCP are thin adapters over it, so both transports enforce identical rules.

Side effects subscribe to the `EventBus` (`events.rs`) as `EventSink` adapters: `WebhookSink` and `AnalyticsSink` in `rungu-api`, wired by `rungu_api::default_event_bus()` for both the HTTP server and `rungu mcp`. A new side effect (e.g. email notifications) is one more sink, not a new call in every handler. Tests use `RecordingSink`.

### rungu-auth

OAuth providers (Google, GitHub, Keycloak), JWT session issuance and validation, Axum middleware (`CurrentUser` extractor). ENV-driven configuration.

### rungu-api

Axum route handlers. Mutations decode the request, call `state.ops`, and map `OpError` to an HTTP status; board reads call `state.ops.queries()` (`BoardQuery`). Also hosts the webhook and analytics event sinks, and the `/mcp` HTTP transport.

### rungu-mcp

MCP tools for AI agents: 26 tools, one transport-independent dispatcher (`handle_message(input, ops, actor)`). Two transports use it: stdio (`rungu mcp`, a local trusted subprocess running as the built-in `mcp@rungu.local` admin) and `POST /mcp` (in `rungu-api`, Bearer `RUNGU_API_KEY`, acting as the `ai-agent` identity). The transport picks the actor; mutating tools call the shared `Operations` and read tools call `BoardQuery`, so REST and MCP apply the same rules. Failures return JSON-RPC errors with a code per kind. See [MCP trust boundary](../integrations/mcp.md#trust-boundary--security).

### rungud

Binary entrypoint. CLI subcommands (`serve`, `project-add`, `project-list`, `healthcheck`, `mcp`). Embeds the SvelteKit SPA via `rust-embed`.

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
→ rungu-core: ops.create_post() — validate, write, publish PostCreated
→ EventBus → WebhookSink + AnalyticsSink (detached tasks)
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
