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

## CLA

All contributions (code, docs, tests, configuration) require a signed
Contributor License Agreement before a pull request can be merged:

- 📋 **Individual?** → [Sign the Individual CLA](https://codecoradev.github.io/cla/?type=individual)
- 🏢 **Contributing on behalf of a company?** → [Sign the Corporate CLA](https://codecoradev.github.io/cla/?type=corporate)

The CLA is a license agreement, not a copyright assignment — you keep
ownership of your work. Signing takes a couple of minutes and is stored
in the [codecoradev/.github](https://github.com/codecoradev/.github)
repository; a bot checks it automatically on every pull request.

## Contributions are unpaid

Contributing to this project is **voluntary and unpaid**. There is no
compensation, payment, bounty, or financial reward of any kind for
contributions — now or in the future. You contribute on your own time,
at your own discretion, because you want to improve the project.

If any paid-contribution program is ever introduced, it will be announced
explicitly and this document will be updated. Until then, assume every
contribution is volunteer work under the Apache-2.0 license terms above.
