# Configuration

All configuration is done via environment variables.

## Server

| Variable | Default | Description |
|----------|---------|-------------|
| `RUNGU_LISTEN` | `0.0.0.0:3000` | HTTP listen address |
| `RUNGU_DB` | `rungu.db` | SQLite database path |
| `RUNGU_CORS_ORIGINS` | `*` (all) | Comma-separated CORS origins |
| `RUNGU_SECURE_COOKIE` | `true` | Set `false` for HTTP (no Secure flag on cookies) |

## Auth (Session)

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_URL` | `http://localhost:3000` | Base URL (used for OAuth redirect URIs) |
| `APP_SECRET` | `dev-secret-change-me` | JWT signing secret (**change in production!**) |

## Google OAuth

Set these to enable Google login:

| Variable | Description |
|----------|-------------|
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GOOGLE_REDIRECT_URI` | Override redirect URI (default: `{APP_URL}/auth/google/callback`) |

## GitHub OAuth

Set these to enable GitHub login:

| Variable | Description |
|----------|-------------|
| `GITHUB_CLIENT_ID` | GitHub OAuth App client ID |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth App client secret |
| `GITHUB_REDIRECT_URI` | Override redirect URI (default: `{APP_URL}/auth/github/callback`) |

## Keycloak OAuth

Set these to enable Keycloak login:

| Variable | Description |
|----------|-------------|
| `KEYCLOAK_URL` | Keycloak base URL (e.g., `https://auth.example.com`) |
| `KEYCLOAK_REALM` | Keycloak realm name |
| `KEYCLOAK_CLIENT_ID` | Client ID for Rungu in Keycloak |
| `KEYCLOAK_CLIENT_SECRET` | Client secret for Rungu in Keycloak |
| `KEYCLOAK_REDIRECT_URI` | Override redirect URI |

## Example .env

```env
# Server
RUNGU_LISTEN=0.0.0.0:3000
RUNGU_DB=/data/rungu.db
APP_SECRET=a1b2c3d4e5f6...

# Auth
APP_URL=https://feedback.example.com

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
```

## Provider Priority

- Empty/unset provider ENV = that provider is **disabled**
- Multiple providers can be active simultaneously
- Users are identified by **email** — same email across providers = same user
- First login creates the user, subsequent logins from any provider link to the same account
