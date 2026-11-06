# CLAUDE.md — Rungu Project Conventions

## Project Overview

Rungu is a lightweight, self-hosted feedback board written in Rust with an embedded SvelteKit SPA. Users submit feature requests, bug reports, and suggestions with voting and commenting. Apache-2.0 licensed.

**Scope:** Feedback collection, voting, and prioritization only. No project management, no kanban, no sprint planning.

## Architecture

- **Single binary** with embedded SPA via `rust-embed`
- **SQLite** (WAL mode, `synchronous=NORMAL`) — single-writer, `current_thread` tokio
- **Multi-provider OAuth** — Google, GitHub, Keycloak (ENV-driven)
- **Email-based identity dedup** — same email = same user across providers
- **JWT session tokens** — signed by APP_SECRET, HttpOnly cookie
- **MCP:** two transports over one dispatcher (`rungu-mcp`): stdio for a local trusted subprocess, and `POST /mcp` over HTTP with the `RUNGU_API_KEY` Bearer key. The transport decides the acting identity; `Operations` enforces the rules
- **REST API:** JSON, CORS, trace logging

## Crate Naming

| Crate | Purpose |
|-------|---------|
| `rungu-proto` | Wire types (Post, Vote, Comment, User, Project, enums) |
| `rungu-core` | Storage trait, config, business logic, SQLite queries |
| `rungu-auth` | Multi-provider OAuth (Google/GitHub/Keycloak), JWT session, middleware |
| `rungu-api` | REST API routes (Axum handlers) |
| `rungu-mcp` | MCP tools (JSON-RPC 2.0), served over stdio and by `rungu-api` at `/mcp` |
| `rungud` | Daemon binary (CLI: `rungu`) |

## Rust Style

- Edition 2024, MSRV 1.88
- `thiserror` for library errors, `anyhow` for application errors
- `tracing` for all logging (no `println!` in library code)
- No `unwrap()` in production code — use `?` or explicit error handling
- `serde` derive on all wire types
- Tests alongside source files (`#[cfg(test)]` modules)
- Release profile: `opt-level=z`, `lto=fat`, `panic=abort`, `strip=true`

## Git Workflow

- **Default branch:** `develop`
- **Release branch:** `main` (mirror, auto-synced via tag)
- **Versioning:** Stay in `0.x.x` indefinitely
- **NEVER push to main directly** — always PR to develop
- **CHANGELOG.md** — version section per release
- Commit messages: conventional format (`feat:`, `fix:`, `chore:`, `docs:`)

## Auth Design

- Providers configured via ENV — empty = disabled
- OAuth callback per provider: `/auth/google/callback`, `/auth/github/callback`, `/auth/keycloak/callback`
- Email dedup: find-or-create user by email on every login
- JWT session signed by APP_SECRET (HS256), 7-day expiry, HttpOnly + Secure cookie
- Roles: `admin` (manage projects, change status) and `member` (submit, vote, comment)
- No username — identity = email + user ID

## Key Design Decisions

| Decision | Status |
|----------|--------|
| Email-based identity dedup | Decided |
| Multi-provider OAuth via ENV | Decided |
| JWT session (not OAuth tokens) | Decided |
| No username, email-only | Decided |
| MCP over stdio and HTTP (`/mcp`, API key); actor comes from the transport | Decided |
| SQLite single-writer | Decided |
| Apache-2.0 license | Decided |

## What Rungu is NOT

- ❌ Project management / kanban
- ❌ Sprint planning
- ❌ Issue tracker (use GitHub/GitLab)
- ❌ Customer support / helpdesk
- ❌ CRM
