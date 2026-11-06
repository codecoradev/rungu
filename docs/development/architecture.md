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

MCP tools for AI agents: 26 tools declared once in `TOOLS` (`tools.rs`), one transport-independent dispatcher (`handle_message(input, ops, actor)`). Two transports use it: stdio (`rungu mcp`, a local trusted subprocess running as the built-in `mcp@rungu.local` admin) and `POST /mcp` (in `rungu-api`, Bearer `RUNGU_API_KEY`, acting as the `ai-agent` identity). The transport picks the actor; mutating tools call the shared `Operations` and read tools call `BoardQuery`, so REST and MCP apply the same rules. Failures return JSON-RPC errors with a code per kind. See [MCP trust boundary](../integrations/mcp.md#trust-boundary--security).

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
→ rungu-mcp: parse JSON-RPC, look the tool up in TOOLS, run its handler
→ rungu-core: BoardQuery.board() (reads) / Operations (writes)
→ stdout: JSON-RPC response
```

## Adding a capability

A capability is something the board can do (create a post, read the roadmap, delete a webhook). The REST API and MCP expose the same ones, and the rules for each live in one place so the two can't drift.

1. **Put the behaviour in `rungu-core`.**
   - A change to data goes in a method on `Operations` (`ops.rs`): existence check, then authorization, then validation, then the store write, then the `DomainEvent`. Rules such as slug format or title length sit next to it (`validation.rs`).
   - A read goes in `BoardQuery` (`query.rs`), which owns slug lookup, paging clamps and filter parsing.
   - Test it at that interface in `crates/rungu-core/tests/` (`ops_test.rs`, `query_test.rs`). These tests cover REST and MCP at once.
2. **Add the REST route.** An Axum handler that decodes the request, calls the method above, and encodes the result, with its `#[utoipa::path]` (include `security(...)` if the route needs a login) and an entry in `openapi.rs`. `OpError` already maps to HTTP statuses.
3. **Add the MCP tool**, or decide it is REST-only.
   - In `crates/rungu-mcp/src/tools.rs`, add one `Tool` to `TOOLS`: name, summary, parameters, `Access`, `Effect`, the REST route it mirrors, and the handler. The handler decodes parameters, calls the same method as the REST handler, and encodes the result. `tools/list`, dispatch and the admin check all come from that entry.
   - If the endpoint should not be a tool (a file upload, a browser login, an email link), add it to `REST_ONLY` with the reason instead.
4. **Document it:** a row in the tools table of `docs/integrations/mcp.md`, and a line in `CHANGELOG.md`.

CI checks that you did all of this. `rungu-api/tests/parity_test.rs` fails if an OpenAPI operation has neither a tool nor a `REST_ONLY` entry, if a tool's declared `Access` disagrees with the route's `security`, or if REST and MCP end a scenario differently (ok, invalid, forbidden, not found, conflict). `rungu-mcp`'s tests fail if the docs miss a tool or state the wrong tool count.

## Deployment

- **Single binary** — no external dependencies
- **Embedded SPA** — SvelteKit build via `rust-embed`
- **SQLite** — zero-config database, WAL mode for concurrent reads
- **Docker** — multi-stage build: Node (frontend) → Rust (backend) → scratch
