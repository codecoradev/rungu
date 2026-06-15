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
| `member` | Can submit posts, vote, and comment |
| `admin` | Can also change post status, delete posts, manage projects |

First user is automatically `admin`. New users default to `member`.

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
