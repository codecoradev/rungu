# OpenID Connect (Gatehouse and others)

Use this to sign people in with any standards-compliant OpenID Connect issuer, for example **Gatehouse** (CodeCora ID). It sits beside Google, GitHub and Keycloak: set it up and a "Continue with …" button appears on the login page.

## 1. Register Rungu as a client at the issuer

Create a confidential client with:

- Redirect URI: `https://your-domain.com/auth/oidc/callback` (exact match)
- Scopes: `openid profile email`
- PKCE S256: on (Rungu always sends it)

In Gatehouse this is an OAuth client in the admin console (`/admin/oauth2`); see Gatehouse's `docs/rp-onboarding-oidc.md`. Copy the client ID and secret.

## 2. Configure Rungu

```env
APP_URL=https://your-domain.com
OIDC_ISSUER=https://identity.example.com/api/auth   # the issuer URL (Gatehouse: <origin>/api/auth)
OIDC_CLIENT_ID=your-client-id
OIDC_CLIENT_SECRET=your-client-secret
OIDC_NAME=CodeCora ID                                # optional: the button label
```

Rungu reads the authorization, token and userinfo endpoints from `{OIDC_ISSUER}/.well-known/openid-configuration`. The issuer URL must be `https://` (loopback excepted for local development), and the document must name that same issuer.

If `OIDC_ISSUER` is set, `OIDC_CLIENT_ID` and `OIDC_CLIENT_SECRET` are required, otherwise Rungu refuses to start. The endpoints are discovered on first use and refreshed hourly, so an issuer that is down when Rungu boots does not stop Rungu; the OIDC button answers 502 until the issuer is back. After the first success, a refresh that fails keeps serving the last good document, and a failed fetch is not retried for 15 seconds, so a down issuer is not hammered. A burst of logins makes one discovery request.

## How a login is decided

- The flow is the authorization-code flow with `state` and **PKCE (S256)**. The issuer must support S256 when it lists its methods.
- Identity comes from the userinfo response. The email must be marked `"email_verified": true`; a missing or false claim refuses the login with HTTP 403 before any account is looked up or created, like the other providers.
- The person is found or created by email (so the same email is the same Rungu user whichever provider they use), and `ADMIN_EMAILS` decides the admin role. Identity is the email, not the issuer's `sub`: the `sub` is recorded as a link but never checked, so if the issuer later reports a different verified email for the same `sub`, the login goes to whoever owns that email.
- The built-in `agent@rungu.local` and `mcp@rungu.local` identities (admins used by the API key and `rungu mcp`) can never be logged into through any provider: such a login is refused with 403, whatever the provider says about the address.

## Using the same issuer for MCP

The same Gatehouse can authenticate remote MCP clients such as Claude connectors: see [MCP](../integrations/mcp.md#connect-from-claude-oauth). Both use the same email-based identity, so a person is one Rungu user in the browser and over MCP.

## Not covered yet

Refresh tokens, RP-initiated logout and back-channel logout. Signing out of Rungu ends the Rungu session only.
