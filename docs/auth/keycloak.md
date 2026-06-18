# Keycloak Setup

Keycloak is optional — use it if you already have a Keycloak instance or want central SSO across multiple applications.

## 1. Create Client in Keycloak

1. Go to your Keycloak Admin Console
2. Select your realm
3. Click **Clients → Create**
4. Client ID: `rungu`
5. Client authentication: **On**
6. Valid redirect URIs: `https://your-domain.com/auth/keycloak/callback/*`
7. Web origins: `https://your-domain.com`

## 2. Get Credentials

After creation:
1. Go to **Clients → rungu → Credentials**
2. Copy the **Client Secret**

## 3. Configure Rungu

```env
APP_URL=https://your-domain.com
KEYCLOAK_URL=https://auth.example.com    # Keycloak base URL (no /auth)
KEYCLOAK_REALM=myorg                      # Your realm name
KEYCLOAK_CLIENT_ID=rungu
KEYCLOAK_CLIENT_SECRET=your-keycloak-secret
```

## Redirect URI

Default: `{KEYCLOAK_URL}/realms/{KEYCLOAK_REALM}` → callback at `{APP_URL}/auth/keycloak/callback`

There is no `KEYCLOAK_REDIRECT_URI` override — the redirect URI is always derived from `APP_URL`.
