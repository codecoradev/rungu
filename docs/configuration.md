# Configuration

All configuration is done via environment variables.

## Server

| Variable | Default | Description |
|----------|---------|-------------|
| `RUNGU_LISTEN` | `0.0.0.0:3000` | HTTP listen address |
| `RUNGU_DB` | `rungu.db` | SQLite database path |
| `DATABASE_URL` | _(unset)_ | Override the database connection. When set, takes precedence over `RUNGU_DB`. Format: `sqlite:path.db` or `postgres://user:pass@host/db`. |
| `RUNGU_CORS_ORIGINS` | _(APP_URL only)_ | Comma-separated CORS origins. Default: only `APP_URL`. Set to `*` to allow all (dev only). |
| `RUNGU_SECURE_COOKIE` | `true` | Set `false` for HTTP (no Secure flag on cookies). Accepts (case-insensitive): `true\|1\|yes\|on`, `false\|0\|no\|off`. Any other value exits with a fatal error — see [Security](#security). |
| `RUST_LOG` | `rungu=info` | Log level (trace, debug, info, warn, error). Supports `tracing_subscriber`'s [`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax. |

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
