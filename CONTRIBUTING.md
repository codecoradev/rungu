# Contributing to Rungu

Thanks for your interest in contributing!

## Development Setup

```bash
# Prerequisites
rustup (Rust 1.88+)
Node.js 22+ (for frontend)

# Clone & build
git clone https://github.com/codecoradev/rungu.git
cd rungu
cargo build

# Frontend
cd web && npm ci && npm run build && cd ..

# Run
cargo run -- --db rungu.db serve --listen 0.0.0.0:3000
```

## Code Style

- Rust: `cargo fmt` + `cargo clippy` must pass
- Frontend: SvelteKit 5 + Tailwind v4 + shadcn-svelte
- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`

## PR Process

1. Fork from `develop`
2. Create feature branch
3. PR to `develop` (not `main`)
