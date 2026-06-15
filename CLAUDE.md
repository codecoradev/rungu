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
- **MCP:** stdio transport only (no TCP) — process isolation replaces auth
- **REST API:** JSON, CORS, trace logging

## Crate Naming

| Crate | Purpose |
|-------|---------|
| `rungu-proto` | Wire types (Post, Vote, Comment, User, Project, enums) |
| `rungu-core` | Storage trait, config, business logic, SQLite queries |
| `rungu-auth` | Multi-provider OAuth (Google/GitHub/Keycloak), JWT session, middleware |
| `rungu-api` | REST API routes (Axum handlers) |
| `rungu-mcp` | MCP server via stdio (JSON-RPC 2.0) |
| `rungud` | Daemon binary (CLI: `rungu`) |

## Rust Style

- Edition 2024, MSRV 1.86
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
| MCP stdio only (no TCP) | Decided |
| SQLite single-writer | Decided |
| Apache-2.0 license | Decided |

## What Rungu is NOT

- ❌ Project management / kanban
- ❌ Sprint planning
- ❌ Issue tracker (use GitHub/GitLab)
- ❌ Customer support / helpdesk
- ❌ CRM
