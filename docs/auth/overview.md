# Auth Overview

Rungu uses multi-provider OAuth for authentication. No username/password — users log in with their Google, GitHub, or Keycloak account.

## How It Works

1. User clicks "Login with Google/GitHub/Keycloak"
2. Redirected to the OAuth provider's consent screen
3. Provider redirects back with an authorization code
4. Rungu exchanges the code for user info (email, name, avatar)
5. User is found or created by email in the local database
6. A JWT session cookie is set (7-day expiry)

## Email-Based Identity

Users are identified by **email address**, not by provider ID. This means:

- A user who logs in via Google with `anaz@example.com` and later via GitHub with the same email → **same account**
- Each login is recorded as an `identity` (for tracking which providers are linked)
- No username system — user ID is internal, email is the display identity

### ⚠️ Verified-email requirement

Account linking by email is only performed when the provider asserts that the user controls the email. Specifically:

- **Google** — reads `email_verified` from the userinfo response.
- **GitHub** — calls `/user/emails` and requires a **primary + verified** email.
- **Keycloak** — reads `email_verified` from the userinfo response. Realm administrators must enforce email verification upstream (this is on by default for the Keycloak registration flow).

Logins presenting an unverified email are **rejected with HTTP 403** before any account lookup or creation happens. This prevents cross-provider account takeover via an attacker-controlled or misconfigured IdP (e.g. a self-hosted Keycloak realm that allows admin-reassignable emails).

If a user sees "Email is not verified by the provider," ask them to verify their email with the provider (Google/GitHub/Keycloak) and retry.

## Provider Setup

| Provider | Setup Guide |
|----------|-------------|
| Google | [Google OAuth Setup](/auth/google) |
| GitHub | [GitHub OAuth Setup](/auth/github) |
| Keycloak | [Keycloak Setup](/auth/keycloak) |

## ENV Reference

```env
APP_URL=https://feedback.example.com     # Base URL for redirect URIs
APP_SECRET=random-32-char-hex             # JWT signing secret

# Enable/disable providers by setting or leaving empty:
GOOGLE_CLIENT_ID=xxx                       # Google OAuth
GOOGLE_CLIENT_SECRET=xxx
GITHUB_CLIENT_ID=xxx                       # GitHub OAuth
GITHUB_CLIENT_SECRET=xxx
KEYCLOAK_URL=https://auth.example.com      # Keycloak (optional)
KEYCLOAK_REALM=myorg
KEYCLOAK_CLIENT_ID=rungu
KEYCLOAK_CLIENT_SECRET=xxx
```

## User Roles

| Role | Description |
|------|-------------|
| `member` | Default role — can submit posts, vote, and comment |
| `admin` | Granted via the `ADMIN_EMAILS` env var — can also change post status, delete posts, and manage projects |

Admins are configured explicitly via the `ADMIN_EMAILS` environment variable (comma-separated email allowlist). There is **no implicit admin grant** — the first user to sign up does **not** become admin, and there is no self-serve privilege escalation path.

New users default to `member`. Existing users who later appear in `ADMIN_EMAILS` are auto-promoted on their next login.

```env
# Grant admin to specific email addresses
ADMIN_EMAILS=owner@example.com,teammate@example.com
```

If `ADMIN_EMAILS` is unset or empty, **no users are admin** (the instance is effectively read-only for status/project management until an admin is configured).

## API: Check Auth Status

```bash
curl -b cookies.txt https://feedback.example.com/api/auth/me
```

```json
{
  "id": "abc123",
  "email": "anaz@example.com",
  "role": "admin",
  "identities": [
    { "provider": "google", "provider_id": "10xxx" },
    { "provider": "github", "provider_id": "12345" }
  ]
}
```
