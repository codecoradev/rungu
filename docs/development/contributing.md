# Contributing

Thanks for your interest in contributing to Rungu!

## Development Setup

```bash
# Prerequisites
rustup (Rust 1.86+)
Node.js 22+

# Clone
git clone https://github.com/codecoradev/rungu.git
cd rungu

# Backend
cargo build
cargo test

# Frontend
cd web && npm install && npm run dev

# Run full stack
cargo run -- serve
```

## Project Structure

```
rungu/
├── crates/
│   ├── rungu-proto/      Wire types
│   ├── rungu-core/       Storage + business logic
│   ├── rungu-auth/       OAuth + JWT session
│   ├── rungu-api/        REST API handlers
│   ├── rungu-mcp/        MCP server
│   └── rungud/           Binary entrypoint
├── web/                  SvelteKit 5 frontend
├── docs/                 VitePress documentation
└── docker/               Docker files
```

## Code Style

- **Rust**: `rustfmt.toml` — 120 char width, max heuristics
- **Clippy**: All warnings must pass
- **Frontend**: SvelteKit 5 runes mode, Tailwind v4, shadcn-svelte

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add threaded comments
fix: vote toggle race condition
docs: update auth setup guide
refactor: extract OAuth provider trait
test: add store integration tests
```

## PR Process

1. Create feature branch from `develop`
2. Make changes + tests
3. Push and open PR targeting `develop`
4. CI must pass: Check, Format, Clippy, Test, Build, Cora Review
5. PR is merged to `develop`
6. Periodic merge: `develop` → `main` for releases

## Branch Protection

- `develop`: PR required, status checks required
- `main`: PR required, status checks required, strict mode (must be up to date)

## Questions?

Open a [GitHub Discussion](https://github.com/codecoradev/rungu/discussions) or create an issue.
