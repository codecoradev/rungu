# Docker

## Quick Start

```bash
docker run -d \
  --name rungu \
  -p 3000:3000 \
  -v rungu-data:/data \
  -e APP_SECRET=$(openssl rand -hex 32) \
  ghcr.io/codecoradev/rungu:latest
```

## Docker Compose

> The Compose snippet below uses `http://localhost:3000` as the default
> `APP_URL` for **local development only**. For any deployment that's reachable
> beyond your own machine, set `APP_URL` to an `https://…` origin and keep
> `RUNGU_SECURE_COOKIE=true` (the default). Plain-HTTP deployments with secure
> cookies will silently fail to send the session cookie and break login.

```yaml
services:
  rungu:
    image: ghcr.io/codecoradev/rungu:latest
    container_name: rungu
    restart: unless-stopped
    ports:
      - "${RUNGU_PORT:-3000}:3000"
    volumes:
      - rungu-data:/data
    environment:
      - RUNGU_DB=/data/rungu.db
      - RUNGU_LISTEN=0.0.0.0:3000
      - APP_URL=${APP_URL:-http://localhost:3000}   # local-dev default; override with https://… for production
      - APP_SECRET=${APP_SECRET:?Set APP_SECRET}

      # Auth providers (empty = disabled)
      - GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID:-}
      - GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET:-}
      - GITHUB_CLIENT_ID=${GITHUB_CLIENT_ID:-}
      - GITHUB_CLIENT_SECRET=${GITHUB_CLIENT_SECRET:-}
      - KEYCLOAK_URL=${KEYCLOAK_URL:-}
      - KEYCLOAK_REALM=${KEYCLOAK_REALM:-}
      - KEYCLOAK_CLIENT_ID=${KEYCLOAK_CLIENT_ID:-}
      - KEYCLOAK_CLIENT_SECRET=${KEYCLOAK_CLIENT_SECRET:-}

volumes:
  rungu-data:
```

## With Reverse Proxy (Traefik)

```yaml
services:
  rungu:
    image: ghcr.io/codecoradev/rungu:latest
    volumes:
      - rungu-data:/data
    environment:
      - APP_URL=https://feedback.example.com
      - APP_SECRET=${APP_SECRET}
      - GOOGLE_CLIENT_ID=${GOOGLE_CLIENT_ID}
      - GOOGLE_CLIENT_SECRET=${GOOGLE_CLIENT_SECRET}
    networks:
      - web
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.rungu.rule=Host(`feedback.example.com`)"
      - "traefik.http.routers.rungu.entrypoints=websecure"
      - "traefik.http.routers.rungu.tls.certresolver=letsencrypt"
      - "traefik.http.services.rungu.loadbalancer.server.port=3000"

volumes:
  rungu-data:

networks:
  web:
    external: true
```

## Build from Source

```bash
docker build -t rungu .

# APP_SECRET is required — the process exits without it. Generate a strong one:
docker run -d \
  -p 3000:3000 \
  -e APP_SECRET="$(openssl rand -hex 32)" \
  rungu
```

## Health Check

The Dockerfile includes a built-in HEALTHCHECK:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["rungu", "healthcheck"]
```
