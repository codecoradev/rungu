# Production Deployment

This guide covers deploying Rungu in production with Docker Compose, Traefik as a reverse proxy, and automatic TLS certificates from Let's Encrypt.

---

## Architecture Overview

Rungu is a Rust backend with an embedded SvelteKit SPA, shipped as a single statically-linked binary inside a `scratch` container. There is no separate frontend server to run — the SPA is embedded at compile time via `rust-embed`.

```
                    ┌──────────────────────────────────┐
                    │           Traefik :443            │
                    │  (Let's Encrypt TLS termination)  │
                    └───────────────┬──────────────────┘
                                    │
                            ┌───────┴───────┐
                            │  Rungu :3000   │
                            │  (single binary│
                            │   + SPA embed) │
                            └───────┬───────┘
                                    │
                         ┌──────────┴──────────┐
                         │                     │
                   ┌─────┴─────┐        ┌──────┴──────┐
                   │ PostgreSQL │        │  Volume     │
                   │  (rec.)    │        │  (uploads)  │
                   └────────────┘        └─────────────┘
```

**Database options:**

| Backend | When to use |
|---------|-------------|
| **PostgreSQL** (recommended) | Production, multi-user, >100k posts, concurrent write load |
| **SQLite** | Small teams, <100 users, single-instance, simpler backups |

SQLite is fully supported for production use. Switch to PostgreSQL if you need higher write concurrency, connection pooling across multiple processes, or managed database hosting.

---

## Prerequisites

- **Docker** 24+ with the Compose v2 plugin
- **Docker Compose** v2 (`docker compose` command, not the legacy `docker-compose`)
- **A domain name** with DNS A records pointing to your server's public IP
  - Example: `feedback.example.com` → `203.0.113.10`
- **Ports 80 and 443** open on the server firewall (Traefik needs both — 80 for the HTTP→HTTPS redirect, 443 for TLS)
- **OAuth provider credentials** set up in advance (see [OAuth Provider Setup](#oauth-provider-setup))

### Verify prerequisites

```bash
docker --version          # Docker version 24+
docker compose version    # Docker Compose version v2.20+
openssl rand -hex 32      # Generate APP_SECRET
```

---

## Docker Compose Setup

The Compose file below is a complete, self-contained production setup. It runs three services: Traefik (reverse proxy with automatic TLS), PostgreSQL, and Rungu.

```yaml
# docker-compose.yml
services:
  # ── Reverse Proxy ──────────────────────────────────────────────────
  traefik:
    image: traefik:v3.2
    container_name: traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
      # Dashboard — remove in production or protect with a firewall
      - "8080:8080"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - traefik-letsencrypt:/letsencrypt        # TLS certificate storage
    command:
      # Entrypoints
      - --entrypoints.web.address=:80
      - --entrypoints.websecure.address=:443
      - --entrypoints.traefik.address=:8080
      # HTTP → HTTPS redirect on the web entrypoint
      - --entrypoints.web.http.redirections.entrypoint.to=websecure
      - --entrypoints.web.http.redirections.entrypoint.scheme=https
      # Docker provider — only enable services with traefik.enable=true
      - --providers.docker=true
      - --providers.docker.exposedbydefault=false
      # Let's Encrypt ACME
      - --certificatesresolvers.letsencrypt.acme.tlschallenge=true
      - --certificatesresolvers.letsencrypt.acme.email=${ACME_EMAIL:?Set ACME_EMAIL}
      - --certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json
      # Dashboard (protect or disable in production)
      - --api.dashboard=true
      - --api.insecure=true
    networks:
      - rungu-net

  # ── PostgreSQL ─────────────────────────────────────────────────────
  postgres:
    image: postgres:17-alpine
    container_name: rungu-postgres
    restart: unless-stopped
    environment:
      - POSTGRES_USER=${POSTGRES_USER:-rungu}
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD:?Set POSTGRES_PASSWORD}
      - POSTGRES_DB=${POSTGRES_DB:-rungu}
    volumes:
      - postgres-data:/var/lib/postgresql/data
    networks:
      - rungu-net
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER:-rungu}"]
      interval: 10s
      timeout: 5s
      retries: 5

  # ── Rungu ──────────────────────────────────────────────────────────
  rungu:
    image: ghcr.io/codecoradev/rungu:latest
    container_name: rungu
    restart: unless-stopped
    depends_on:
      postgres:
        condition: service_healthy
    volumes:
      - rungu-uploads:/data/uploads         # Attachment storage
    environment:
      # ── Database (PostgreSQL for production) ──
      - DATABASE_URL=postgres://${POSTGRES_USER:-rungu}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB:-rungu}

      # ── Server ──
      - RUNGU_LISTEN=0.0.0.0:3000
      - RUNGU_TRUST_PROXY=true
      - RUST_LOG=rungu=info

      # ── Auth / Session ──
      - APP_URL=${APP_URL:?Set APP_URL}
      - APP_SECRET=${APP_SECRET:?Set APP_SECRET}
      - ADMIN_EMAILS=${ADMIN_EMAILS:-}

      # ── CORS & Rate Limiting ──
      - RUNGU_CORS_ORIGINS=${RUNGU_CORS_ORIGINS:-}
      - RUNGU_RATE_LIMIT_PER_MIN=${RUNGU_RATE_LIMIT_PER_MIN:-300}
      - RUNGU_AUTH_RATE_LIMIT_PER_MIN=${RUNGU_AUTH_RATE_LIMIT_PER_MIN:-30}

      # ── File Storage ──
      - RUNGU_STORAGE_DIR=/data/uploads

      # ── Google OAuth (leave empty to disable) ──
      - GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID:-}
      - GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET:-}

      # ── GitHub OAuth (leave empty to disable) ──
      - GITHUB_CLIENT_ID=${GITHUB_CLIENT_ID:-}
      - GITHUB_CLIENT_SECRET=${GITHUB_CLIENT_SECRET:-}

      # ── Keycloak OIDC (leave empty to disable) ──
      - KEYCLOAK_URL=${KEYCLOAK_URL:-}
      - KEYCLOAK_REALM=${KEYCLOAK_REALM:-}
      - KEYCLOAK_CLIENT_ID=${KEYCLOAK_CLIENT_ID:-}
      - KEYCLOAK_CLIENT_SECRET=${KEYCLOAK_CLIENT_SECRET:-}

      # ── Sentry (optional, leave empty to disable) ──
      - SENTRY_DSN=${SENTRY_DSN:-}
    labels:
      - "traefik.enable=true"
      # Router: HTTPS
      - "traefik.http.routers.rungu.rule=Host(`${DOMAIN}`)"
      - "traefik.http.routers.rungu.entrypoints=websecure"
      - "traefik.http.routers.rungu.tls.certresolver=letsencrypt"
      # Service port inside the container
      - "traefik.http.services.rungu.loadbalancer.server.port=3000"
      # Middleware: trust forwarded headers from Traefik
      - "traefik.http.routers.rungu.middlewares=rungu-headers"
      - "traefik.http.middlewares.rungu-headers.headers.stsecsseconds=31536000"
      - "traefik.http.middlewares.rungu-headers.headers.customframeoptionsvalue=SAMEORIGIN"
    networks:
      - rungu-net

volumes:
  postgres-data:
  rungu-uploads:
  traefik-letsencrypt:

networks:
  rungu-net:
```

### Environment File

Create a `.env` file in the same directory as your `docker-compose.yml`:

```env
# ── Required ──────────────────────────────────────────────────────────
DOMAIN=feedback.example.com
APP_URL=https://feedback.example.com
APP_SECRET=<output of: openssl rand -hex 32>
ACME_EMAIL=admin@example.com

# ── PostgreSQL ────────────────────────────────────────────────────────
POSTGRES_USER=rungu
POSTGRES_PASSWORD=<strong random password>
POSTGRES_DB=rungu

# ── Auth (optional — leave empty to disable) ──────────────────────────
ADMIN_EMAILS=owner@example.com
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GITHUB_CLIENT_ID=
GITHUB_CLIENT_SECRET=
KEYCLOAK_URL=
KEYCLOAK_REALM=
KEYCLOAK_CLIENT_ID=
KEYCLOAK_CLIENT_SECRET=

# ── Optional tuning ───────────────────────────────────────────────────
# RUNGU_CORS_ORIGINS=https://feedback.example.com,https://staging.example.com
# RUNGU_RATE_LIMIT_PER_MIN=300
# RUNGU_AUTH_RATE_LIMIT_PER_MIN=30

# ── Sentry (optional) ─────────────────────────────────────────────────
# SENTRY_DSN=https://xxx@o123.ingest.sentry.io/456
```

> **Never commit `.env` to version control.** Add it to `.gitignore`. The `.env.example` in the repository is the canonical reference for all variables.

### Using SQLite instead of PostgreSQL

If you prefer SQLite (smaller footprint, simpler backups), replace the `DATABASE_URL` line and remove the `postgres` service:

```yaml
    environment:
      # Use SQLite instead of PostgreSQL:
      - RUNGU_DB=/data/rungu.db
      # Remove: DATABASE_URL=postgres://...
    volumes:
      - rungu-data:/data            # SQLite DB + uploads
      # Remove: rungu-uploads:/data/uploads

volumes:
  rungu-data:                        # Replaces postgres-data + rungu-uploads
```

---

## Traefik Configuration

The Traefik settings are embedded directly in the `command:` list and Docker labels above. Here's what each piece does:

### Entrypoints

| Entrypoint | Port | Purpose |
|------------|------|---------|
| `web` | 80 | HTTP — immediately redirected to HTTPS |
| `websecure` | 443 | HTTPS — TLS terminated here, traffic forwarded to Rungu |
| `traefik` | 8080 | Dashboard API — restrict access or remove in production |

### HTTP → HTTPS Redirect

All traffic on port 80 is redirected to HTTPS automatically:

```
--entrypoints.web.http.redirections.entrypoint.to=websecure
--entrypoints.web.http.redirections.entrypoint.scheme=https
```

### Let's Encrypt TLS

Traefik uses the TLS-ALPN-01 challenge to obtain certificates automatically. Certificates are stored in `/letsencrypt/acme.json` (persisted via the `traefik-letsencrypt` volume).

- Set `ACME_EMAIL` to a valid email — Let's Encrypt sends expiry warnings there.
- The first certificate request takes a few seconds. During that window, you may see TLS errors in the browser — this is normal.

### Rungu Service Labels

```yaml
labels:
  - "traefik.enable=true"
  - "traefik.http.routers.rungu.rule=Host(`feedback.example.com`)"
  - "traefik.http.routers.rungu.entrypoints=websecure"
  - "traefik.http.routers.rungu.tls.certresolver=letsencrypt"
  - "traefik.http.services.rungu.loadbalancer.server.port=3000"
```

Key points:
- `exposedbydefault=false` is set globally, so only services with `traefik.enable=true` are proxied.
- The `Host()` rule must match your `DOMAIN` and `APP_URL` exactly.
- The load balancer port is `3000` — the port Rungu listens on inside the container.

### Dashboard Security

The Traefik dashboard on `:8080` is enabled for convenience. **For production, either:**

1. **Remove the dashboard entirely** — delete `--api.dashboard=true`, `--api.insecure=true`, and the `8080` port mapping.
2. **Protect it** with Traefik's basic auth middleware:

```yaml
labels:
  - "traefik.http.routers.dashboard.rule=Host(`traefik.example.com`)"
  - "traefik.http.routers.dashboard.entrypoints=websecure"
  - "traefik.http.routers.dashboard.tls.certresolver=letsencrypt"
  - "traefik.http.routers.dashboard.middlewares=dashboard-auth"
  - "traefik.http.middlewares.dashboard-auth.basicauth.users=admin:$$apr1$$..."
```

> Generate the password hash with: `htpasswd -nB admin | sed 's/\$/&/g'` (the `$$` escaping is required in Compose labels).

---

## Environment Variables

All variables are documented in detail in [Configuration](/configuration). Below is a summary focused on production deployment.

### Database

| Variable | Example | Notes |
|----------|---------|-------|
| `DATABASE_URL` | `postgres://rungu:pass@postgres:5432/rungu` | Takes precedence over `RUNGU_DB`. Use for PostgreSQL. |
| `RUNGU_DB` | `/data/rungu.db` | SQLite path. Used when `DATABASE_URL` is unset. |

### Application

| Variable | Required | Example | Notes |
|----------|----------|---------|-------|
| `APP_URL` | ✅ Yes | `https://feedback.example.com` | Used for OAuth redirect URIs and CORS defaults. Must be `https://` in production. |
| `APP_SECRET` | ✅ Yes | _(64-char hex)_ | JWT signing secret. Generate with `openssl rand -hex 32`. Never reuse across deployments. |
| `ADMIN_EMAILS` | Recommended | `owner@example.com` | Comma-separated. Without this, no users get admin role. |
| `RUNGU_SECURE_COOKIE` | Leave default | `true` (default) | Keeps `Secure` flag on session cookies. Don't change in HTTPS production. |

### CORS & Rate Limiting

| Variable | Default | Notes |
|----------|---------|-------|
| `RUNGU_CORS_ORIGINS` | _(APP_URL only)_ | Comma-separated extra origins. Leave unset to allow only `APP_URL`. Never use `*` in production. |
| `RUNGU_RATE_LIMIT_PER_MIN` | `300` | Max API requests/min per IP. `0` disables. |
| `RUNGU_AUTH_RATE_LIMIT_PER_MIN` | `30` | Max auth requests/min per IP. `0` disables. |
| `RUNGU_TRUST_PROXY` | `false` | Set to `true` behind Traefik so rate limiting uses the real client IP from `X-Forwarded-For`. |

### OAuth Providers

| Variable | Provider | Notes |
|----------|----------|-------|
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | Google | Empty = disabled |
| `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET` | GitHub | Empty = disabled |
| `KEYCLOAK_URL` / `KEYCLOAK_REALM` / `KEYCLOAK_CLIENT_ID` / `KEYCLOAK_CLIENT_SECRET` | Keycloak | All four required to enable. Empty = disabled. |

See [OAuth Provider Setup](#oauth-provider-setup) for redirect URI configuration.

### Error Monitoring

| Variable | Default | Notes |
|----------|---------|-------|
| `SENTRY_DSN` | _(unset)_ | If set, Rungu sends errors and performance traces to Sentry. If unset, Sentry is a no-op — no events are sent and no network calls are made. |

---

## OAuth Provider Setup

Rungu derives redirect URIs automatically from `APP_URL`. There are no `*_REDIRECT_URI` overrides.

The pattern is always:

```
{APP_URL}/auth/{provider}/callback
```

### Google

1. Go to [Google Cloud Console → Credentials](https://console.cloud.google.com/apis/credentials)
2. Create an OAuth 2.0 Client ID (Application type: **Web application**)
3. Add the authorized redirect URI:

   ```
   https://feedback.example.com/auth/google/callback
   ```

4. Set in `.env`:

   ```env
   GOOGLE_CLIENT_ID=xxxx.apps.googleusercontent.com
   GOOGLE_CLIENT_SECRET=GOCSPX-xxxx
   ```

See [Google OAuth Setup](/auth/google) for the full guide.

### GitHub

1. Go to [GitHub → Settings → Developer settings → OAuth Apps](https://github.com/settings/developers)
2. Create a new OAuth App
3. Set the **Authorization callback URL**:

   ```
   https://feedback.example.com/auth/github/callback
   ```

4. Set in `.env`:

   ```env
   GITHUB_CLIENT_ID=Ov23xxxx
   GITHUB_CLIENT_SECRET=xxxx
   ```

See [GitHub OAuth Setup](/auth/github) for the full guide.

### Keycloak

1. In your Keycloak Admin Console, create a client for Rungu
2. Set the **Valid Redirect URI**:

   ```
   https://feedback.example.com/auth/keycloak/callback
   ```

3. Set in `.env`:

   ```env
   KEYCLOAK_URL=https://auth.example.com
   KEYCLOAK_REALM=myorg
   KEYCLOAK_CLIENT_ID=rungu
   KEYCLOAK_CLIENT_SECRET=xxxx
   ```

4. Ensure your Keycloak realm enforces email verification — Rungu rejects logins with unverified emails.

See [Keycloak Setup](/auth/keycloak) for the full guide.

---

## First Deployment

```bash
# 1. Create the directory and files
mkdir -p /opt/rungu && cd /opt/rungu

# 2. Create docker-compose.yml (copy from above)
# 3. Create .env (copy from above, fill in real values)

# 4. Generate a strong APP_SECRET
echo "APP_SECRET=$(openssl rand -hex 32)"

# 5. Start everything
docker compose up -d

# 6. Watch the logs — Rungu runs migrations on first boot
docker compose logs -f rungu
```

On first boot, Rungu automatically runs database migrations. You should see a log line like:

```
INFO rungu: Database migrations complete
```

### Verify the deployment

```bash
# Check all services are running
docker compose ps

# Check Rungu health
curl -sf https://feedback.example.com/api/health && echo "OK"

# Check Traefik is proxying
curl -sI https://feedback.example.com | head -1
```

---

## Backup Strategy

### PostgreSQL

Use `pg_dump` from inside the container:

```bash
# Manual backup
docker compose exec postgres pg_dump -U rungu rungu > backup-$(date +%Y%m%d).sql

# Restore
docker compose exec -T postgres psql -U rungu rungu < backup-20250101.sql
```

**Automated daily backup** (cron):

```bash
# Add to crontab — runs at 3:00 AM daily
0 3 * * * cd /opt/rungu && docker compose exec -T postgres pg_dump -U rungu rungu | gzip > /backups/rungu-$(date +\%Y\%m\%d).sql.gz
```

Keep at least 7–30 days of backups. Rotate with `logrotate` or a simple cleanup:

```bash
# Delete backups older than 30 days
find /backups -name "rungu-*.sql.gz" -mtime +30 -delete
```

### SQLite

If using SQLite, the database is a single file at `/data/rungu.db`:

```bash
# Safe copy (SQLite handles concurrent reads)
docker compose exec rungu cat /data/rungu.db > backup-$(date +%Y%m%d).db

# Or use the built-in .backup command (safer for WAL mode)
docker compose exec rungu sqlite3 /data/rungu.db ".backup /data/backup.db"
```

### Attachments

Uploaded files are stored in the volume mounted at `/data/uploads`. Back them up alongside the database:

```bash
# Tar the uploads volume
docker compose exec rungu tar czf - /data/uploads > uploads-$(date +%Y%m%d).tar.gz
```

---

## Updating

```bash
cd /opt/rungu

# 1. Pull the latest image
docker compose pull

# 2. Apply changes (recreates containers if the image changed)
docker compose up -d

# 3. Verify
docker compose ps
docker compose logs --tail=50 rungu
```

Rungu runs migrations automatically on every boot — no manual migration step is needed. Migrations are forward-only and designed to be safe for rolling restarts.

### Pinning to a specific version

For reproducibility, pin the image tag instead of using `latest`:

```yaml
services:
  rungu:
    image: ghcr.io/codecoradev/rungu:v0.2.0   # instead of :latest
```

See [GitHub Releases](https://github.com/codecoradev/rungu/releases) for available tags.

### Rollback

If an update causes issues, roll back to the previous version:

```bash
# Pin the previous known-good version
sed -i 's|rungu:latest|rungu:v0.1.5|' docker-compose.yml
docker compose up -d
```

> Database migrations are forward-only. If a newer version ran migrations, downgrading may require manually reverting schema changes. Always test updates on a staging environment first.

---

## Troubleshooting

### CORS errors in the browser

**Symptom:** Browser console shows `Access-Control-Allow-Origin` errors, or API requests fail after login.

**Cause:** The requesting origin doesn't match `APP_URL`, and `RUNGU_CORS_ORIGINS` isn't set or doesn't include the origin.

**Fix:**

1. Check that `APP_URL` matches your domain exactly (including `https://`, no trailing slash).
2. If the frontend is served from a different origin, add it:

   ```env
   RUNGU_CORS_ORIGINS=https://feedback.example.com,https://www.feedback.example.com
   ```

3. Never use `RUNGU_CORS_ORIGINS=*` in production.

### OAuth redirect loop / "redirect_uri_mismatch"

**Symptom:** Clicking "Login with Google" opens the provider's page but immediately errors or loops back.

**Cause:** The redirect URI registered in the provider console doesn't match `{APP_URL}/auth/{provider}/callback`.

**Fix:**

1. Check `APP_URL` in your `.env` — it must be the production HTTPS URL.
2. Verify the redirect URI in the provider console matches exactly:

   ```
   https://feedback.example.com/auth/google/callback
   ```

   No trailing slash, correct protocol (`https://`), correct domain.

3. The `APP_URL` change requires a container restart: `docker compose restart rungu`.

### Login works but session isn't saved (cookie dropped)

**Symptom:** OAuth login succeeds, but the user appears unauthenticated immediately after.

**Cause:** `RUNGU_SECURE_COOKIE=true` (the default) sets the `Secure` flag on the session cookie. If Traefik isn't terminating TLS properly (e.g., the browser connects over HTTP), the cookie is silently dropped.

**Fix:** Ensure `APP_URL` starts with `https://` and Traefik is handling TLS on port 443. Do not set `RUNGU_SECURE_COOKIE=false` in production.

### Migration failure on startup

**Symptom:** Rungu container exits immediately after starting. Logs show a migration error.

**Fix:**

1. Check the full error:

   ```bash
   docker compose logs rungu
   ```

2. If using PostgreSQL, verify the database is reachable:

   ```bash
   docker compose exec postgres pg_isready -U rungu
   ```

3. If the database is unreachable, ensure `depends_on` with `condition: service_healthy` is set on the `rungu` service (it is in the Compose file above).

4. **Never manually edit the migration history table (`_sqlx_migrations`)** unless directed by a maintainer.

### Traefik shows 404 for the domain

**Symptom:** Visiting `https://feedback.example.com` returns a Traefik 404 page.

**Fix:**

1. Verify the `Host()` rule in Rungu's labels matches your domain:

   ```yaml
   - "traefik.http.routers.rungu.rule=Host(`feedback.example.com`)"
   ```

2. Check that `traefik.enable=true` is set on the Rungu service.

3. Verify Traefik can see the container:

   ```bash
   docker compose exec traefik wget -qO- http://localhost:8080/api/rawdata | grep rungu
   ```

### Let's Encrypt certificate fails to issue

**Symptom:** Browser shows a self-signed or expired certificate, or Traefik logs show ACME errors.

**Fix:**

1. Ensure port 443 is reachable from the internet (Let's Encrypt connects to your server for TLS-ALPN-01 challenge).
2. Verify DNS A record points to the correct IP:

   ```bash
   dig feedback.example.com +short
   ```

3. Check `ACME_EMAIL` is set and valid in `.env`.
4. Check Traefik logs for ACME errors:

   ```bash
   docker compose logs traefik | grep -i acme
   ```

5. Rate limits: Let's Encrypt allows 5 duplicate certificates per week. If you hit rate limits during testing, use the [staging environment](https://letsencrypt.org/docs/staging-environment/) first.

### Rate limiting triggers too aggressively

**Symptom:** API returns `429 Too Many Requests` for legitimate users.

**Fix:** Increase the limits in `.env`:

```env
RUNGU_RATE_LIMIT_PER_MIN=600
RUNGU_AUTH_RATE_LIMIT_PER_MIN=60
```

Also verify `RUNGU_TRUST_PROXY=true` so that rate limiting uses the real client IP from Traefik's `X-Forwarded-For` header, not the proxy's internal IP.
