# Installation

## Prerequisites

- **Rust** 1.88+ (recommended: use [rustup](https://rustup.rs))
- **Node.js** 22+ (only needed for frontend development)

## Download Binary

Download from [GitHub Releases](https://github.com/codecoradev/rungu/releases):

```bash
# Linux (x86_64)
curl -fsSL https://github.com/codecoradev/rungu/releases/latest/download/rungu-linux-amd64 -o rungu
chmod +x rungu

# macOS (Apple Silicon)
curl -fsSL https://github.com/codecoradev/rungu/releases/latest/download/rungu-darwin-arm64 -o rungu
chmod +x rungu
```

## Docker

```bash
docker pull ghcr.io/codecoradev/rungu:latest

# Generate a strong APP_SECRET — this signs JWT session tokens and MUST be unique
# per deployment. Never reuse or commit a real secret.
docker run -d \
  -p 3000:3000 \
  -v rungu-data:/data \
  -e APP_SECRET="$(openssl rand -hex 32)" \
  -e GOOGLE_CLIENT_ID=xxx \
  -e GOOGLE_CLIENT_SECRET=xxx \
  ghcr.io/codecoradev/rungu:latest
```

## From Source

```bash
git clone https://github.com/codecoradev/rungu.git
cd rungu
cargo build --release

# Binary at target/release/rungu
./target/release/rungu serve
```

## Quick Start

1. **Create a project:**
   ```bash
   ./rungu project-add "My App"
   ```

2. **Start the server:**
   ```bash
   ./rungu serve --db feedback.db
   ```

3. **Open in browser:**
   ```
   http://localhost:3000
   ```

4. **Add OAuth providers** (optional — see [Configuration](/configuration)):
   ```bash
   export GOOGLE_CLIENT_ID=xxx.apps.googleusercontent.com
   export GOOGLE_CLIENT_SECRET=GOCSPX-xxx
   ./rungu serve
   ```

## Next Steps

- [Configuration](/configuration) — all ENV variables
- [Auth Setup](/auth/overview) — Google, GitHub, Keycloak
- [CLI Reference](/cli-reference) — all commands
- [Docker Guide](/docker) — Docker Compose setup
