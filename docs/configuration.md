# Configuration

All configuration is done via environment variables.

## Server

| Variable | Default | Description |
|----------|---------|-------------|
| `RUNGU_LISTEN` | `0.0.0.0:3000` | HTTP listen address |
| `RUNGU_DB` | `rungu.db` | SQLite database path |
| `DATABASE_URL` | _(unset)_ | Override the database connection. When set, takes precedence over `RUNGU_DB`. Format: `sqlite:path.db` or `postgres://user:pass@host/db`. |
| `RUNGU_CORS_ORIGINS` | _(APP_URL only)_ | Comma-separated CORS origins. Default: only `APP_URL`. Set to `*` to allow all (dev only). |
| `RUNGU_RATE_LIMIT_PER_MIN` | `300` | Max `/api/*` requests per minute per client IP (fixed window). `0` disables the limiter. Client IP is taken from `X-Forwarded-For` (first hop) when present, else the socket address. |
| `RUNGU_AUTH_RATE_LIMIT_PER_MIN` | `30` | Max `/auth/*` requests per minute per client IP. Stricter than the API limiter to blunt OAuth/login abuse. `0` disables it. |
| `RUNGU_TRUST_PROXY` | `false` | Honor `X-Forwarded-For` when resolving rate-limit client IPs. Enable only behind a trusted reverse proxy that overwrites the header; otherwise clients can spoof it. When `false`, the socket address is used. |
| `RUNGU_SECURE_COOKIE` | `true` | Set `false` for HTTP (no Secure flag on cookies). Accepts (case-insensitive): `true\|1\|yes\|on`, `false\|0\|no\|off`. Any other value exits with a fatal error — see [Security](#security). |
| `RUNGU_API_KEY` | _(unset)_ | Machine access key for AI agents. When set, `Authorization: Bearer <key>` authenticates as the synthetic **ai-agent** admin user on all `/api` routes and `POST /mcp` (MCP over HTTP). Constant-time compared; unset = machine access disabled. |
| `RUST_LOG` | `rungu=info` | Log level (trace, debug, info, warn, error). Supports `tracing_subscriber`'s [`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax. |

## White-label Branding

Present Rungu under your own brand. See [issue #185](https://github.com/codecoradev/rungu/issues/185).

| Variable | Default | Description |
|----------|---------|-------------|
| `RUNGU_INSTANCE_NAME` | `Rungu` | Brand name shown in the header, page titles, login page, and embed board. |
| `RUNGU_LOGO_URL` | _(unset)_ | Logo shown next to the brand name in the SPA header. |
| `RUNGU_FOOTER_TEXT` | _(unset)_ | Footer line shown **only** when the Powered-by badge is removed by a license. |
| `RUNGU_LICENSE_KEY` | _(unset)_ | Polar license key. A valid key removes the "Powered by Rungu" badge. |
| `RUNGU_LICENSE_ORG_ID` | _(unset)_ | Polar organization id the license belongs to (required with `RUNGU_LICENSE_KEY`). |

Branding is instance-level and ENV-driven — no restart-free admin UI by design. The
"Powered by Rungu" badge (SPA footer + embed board) is the OSS growth loop and is **not**
removable via env; only a valid license hides it. Licensing: **$49 per major version**
(minor + patch free forever) or **$70 lifetime**. The gate is intentionally soft: an
invalid or expired license only makes the badge reappear — the board itself is never
throttled or locked.

## AI Remote Access (API Key + MCP over HTTP)

Let AI agents (Claude Code, Cursor, custom agents) control Rungu remotely with full admin power — no interactive OAuth needed.

```bash
# Server: set a strong key (generate with: openssl rand -hex 32)
RUNGU_API_KEY=<your-key>
```

**REST** — any endpoint, including admin:
```bash
curl -H "Authorization: Bearer <key>" https://host/api/admin/analytics/myslug?days=30
```

**MCP over HTTP** — same 26 tools as local stdio:
```json
{ "mcpServers": { "rungu": {
    "url": "https://host/mcp",
    "headers": { "Authorization": "Bearer <key>" }
} } }
```

Behavior: the key identifies the synthetic `ai-agent` user (auto-created as admin at startup — agent-created posts/comments have a valid author). A presented-but-wrong key is always 401 on privileged endpoints and never grants admin power; public reads treat it as anonymous. Rotation = change the env and restart. Unset = machine access fully disabled (previous behavior).

## Auth (Session)

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_URL` | `http://localhost:3000` | Base URL. Used to construct OAuth redirect URIs as `{APP_URL}/auth/{provider}/callback`. |
| `APP_SECRET` | _(required)_ | JWT signing secret. **Must be set** — generate with `openssl rand -hex 32`. Process exits if not set. |
| `ADMIN_EMAILS` | _(empty)_ | Comma-separated email allowlist that receives the `admin` role. Without this, no users are admins (status/project management is read-only). Example: `ADMIN_EMAILS=owner@example.com,teammate@example.com`. |

## Google OAuth

Set these to enable Google login:

| Variable | Description |
|----------|-------------|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |

Redirect URI is constructed automatically as `{APP_URL}/auth/google/callback` — register that exact URL in the Google Cloud Console. There is no `*_REDIRECT_URI` override.

## GitHub OAuth

Set these to enable GitHub login:

| Variable | Description |
|----------|-------------|
| `GITHUB_CLIENT_ID` | GitHub OAuth App client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth App client secret |

The `user:email` scope is requested automatically. Redirect URI is constructed automatically as `{APP_URL}/auth/github/callback` — register that in your GitHub OAuth App settings.

## Keycloak OAuth

Set these to enable Keycloak login:

| Variable | Description |
|----------|-------------|
| `KEYCLOAK_URL` | Keycloak base URL (e.g., `https://auth.example.com`) |
| `KEYCLOAK_REALM` | Keycloak realm name |
| `KEYCLOAK_CLIENT_ID` | Client ID for Rungu in Keycloak |
| `KEYCLOAK_CLIENT_SECRET` | Client secret for Rungu in Keycloak |

Redirect URI is constructed automatically as `{APP_URL}/auth/keycloak/callback`. Realm administrators must enforce email verification upstream for email-based account linking to work.

## Example .env

```env
# Server
RUNGU_LISTEN=0.0.0.0:3000
RUNGU_DB=/data/rungu.db
RUST_LOG=rungu=info

# Auth
APP_URL=https://feedback.example.com
APP_SECRET=a1b2c3d4e5f6...   # generate with: openssl rand -hex 32
ADMIN_EMAILS=owner@example.com

# Google
GOOGLE_CLIENT_ID=123.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=GOCSPX-abc

# GitHub
GITHUB_CLIENT_ID=Iv1.abc
GITHUB_CLIENT_SECRET=def123

# Keycloak (optional — only if you have Keycloak)
# KEYCLOAK_URL=https://auth.example.com
# KEYCLOAK_REALM=myorg
# KEYCLOAK_CLIENT_ID=rungu
# KEYCLOAK_CLIENT_SECRET=xyz789

# CORS — comma-separated, default = APP_URL only. Set to * for dev.
# RUNGU_CORS_ORIGINS=https://feedback.example.com,https://staging.example.com

# Set to false only for local HTTP development. Default true.
# RUNGU_SECURE_COOKIE=false
```

See [`.env.example`](https://github.com/codecoradev/rungu/blob/develop/.env.example) for the canonical reference.

## Provider Behavior

- Empty/unset provider ENV = that provider is **disabled**.
- Multiple providers can be active simultaneously.
- Users are identified by **email** — same verified email across providers links to the same account.
- The **first login** for an email creates the user (role `member` unless the email is in `ADMIN_EMAILS`). Subsequent logins from any provider reuse the existing account.
- The user role is re-evaluated on every login: add an email to `ADMIN_EMAILS` and the user is auto-promoted on their next login.

## Security

- **Verified-email gate.** Accounts are linked by email **only** when the provider asserts `email_verified: true`. Google and Keycloak expose this via the standard `email_verified` userinfo claim; GitHub's verification is determined from the `/user/emails` endpoint (primary + verified). Logins with an unverified email are rejected with HTTP 403 before any DB write — this prevents cross-provider takeover via untrusted IdPs.
- **`RUNGU_SECURE_COOKIE` is strict.** A typo like `RUNGU_SECURE_COOKIE=False` (capital F) will exit at startup with an actionable error rather than silently enabling secure cookies and breaking local HTTP login.
- **`APP_SECRET` is required and must be unique.** It signs JWT session tokens — never reuse across deployments, never commit a real value.

See [Auth Overview](/auth/overview) for the full identity model.

## File Storage (Attachments)

| Variable | Default | Description |
|----------|---------|-------------|
| `STORAGE_DRIVER` | `fs` | Storage backend: `fs` (filesystem) or `s3` (S3-compatible, planned) |
| `RUNGU_STORAGE_DIR` | `./uploads` | Directory for uploaded files (filesystem driver only) |

**Docker:** Set `RUNGU_STORAGE_DIR=/data/uploads` to use the persisted volume.

**S3-compatible (MinIO, R2, AWS S3):** The `s3` driver is accepted but not yet implemented. It will be available in v0.3.

See [Attachments](/features/attachments) for format support and security details.
