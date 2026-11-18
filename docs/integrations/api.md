# REST API

Full REST API for integration with dashboards, widgets, or custom tools.

## Swagger UI

Interactive API documentation is available at runtime:

```
http://localhost:3000/swagger-ui
```

OpenAPI 3.1 spec: `http://localhost:3000/api-docs/openapi.json`

## Base URL

```
https://your-domain.com/api
```

## Authentication

All mutating endpoints require authentication via session cookie (set by OAuth login).

```
Cookie: session=<JWT token>
```

Servers can send `Authorization: Bearer <credential>` instead, see below.

### Bearer credentials

A request with an `Authorization: Bearer` header is judged by that header alone and never falls back to a cookie. The first category that fits decides, and a wrong value is a `401`; it is never retried as another category:

1. **The API key**: exactly `RUNGU_API_KEY` (constant-time comparison). Acts as the `ai-agent` admin.
2. **`rgk_` prefix**: reserved for integration keys, which Rungu does not support yet. Always `401`; the issuer is never consulted.
3. **Three dot-separated segments**: an issuer access token (below), when issuer tokens are enabled for REST. Otherwise `401`.
4. **Anything else**: `401`.

With neither `RUNGU_API_RESOURCE_URL` nor an issuer configured, only step 1 can succeed and nothing else changes for you.

### Issuer access tokens

A product backend can call the API with the user's own access token from your OpenID Connect issuer, so Rungu knows who the person is without trusting the product's word for it. Enable it with `RUNGU_MCP_OAUTH_ISSUER` and `RUNGU_API_RESOURCE_URL` (see [Configuration](../configuration.md)).

```
curl -X POST https://rungu.example.com/api/projects/my-app/posts \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title": "Dark mode"}'
```

The token must be a JWT from the issuer (for example `https://identity.example.com`) that:

- is signed with an RS256/384/512 or ES256/384 key from the issuer's JWKS, with `iss` equal to the issuer, `exp` and `nbf` honoured, and a `sub`;
- has `aud` containing exactly `RUNGU_API_RESOURCE_URL`. **This is a different value from the `/mcp` audience** (`RUNGU_MCP_RESOURCE_URL`): a token minted for `/mcp` is refused here, and one minted for REST is refused on `/mcp`. Ask the issuer for a token whose audience is the REST resource;
- names a verified email, either as the `email` claim or through the issuer's `userinfo` endpoint, with `email_verified: true`. A missing claim is refused;
- is not sender-constrained (no `cnf`: DPoP and mTLS-bound tokens are refused) and not opaque.

The person is found or created by email, as for a web login. **Role:** requests made with an issuer token always run as `member`, even when the email is in `ADMIN_EMAILS` and even if the stored user is an admin (the stored role is not changed). The token is forwarded through third-party backends and is not bound to a project, so it never carries admin rights. Admin actions need the web session or `RUNGU_API_KEY`. Rows the person creates are authored by their user id.

The built-in `agent@rungu.local` and `mcp@rungu.local` identities can never be claimed this way.

| Status | When |
|--------|------|
| `401` with `WWW-Authenticate: Bearer error="invalid_token"` | Bad signature, wrong issuer, wrong audience (including a `/mcp` token), expired or not yet valid, opaque or sender-constrained token, unverified or reserved email, or a token the issuer refuses at `userinfo` |
| `503` | The issuer's keys, metadata or `userinfo` could not be fetched. Keep the token and retry |
| `401` (no challenge) | Issuer tokens are not enabled, or an `rgk_` or malformed Bearer |

Rungu does not publish RFC 9728 protected-resource metadata for the REST resource, so the challenge carries no `resource_metadata` link.

## Endpoints

### Projects

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/projects` | No | List all projects |
| POST | `/projects` | Admin | Create project |
| GET | `/projects/{slug}` | No | Project detail |
| PATCH | `/projects/{slug}` | Admin | Update project |
| DELETE | `/projects/{slug}` | Admin | Delete project |

### Posts

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/projects/{slug}/posts` | No | List posts (sort, filter, search, paginate) |
| POST | `/projects/{slug}/posts` | Member | Create post |
| GET | `/posts/{id}` | No | Post detail, plus `permissions` (`can_edit`, `can_moderate`) for the caller; both `false` when not signed in |
| PATCH | `/posts/{id}` | Admin | Update status/category |
| DELETE | `/posts/{id}` | Admin/Creator | Delete post |

### Votes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/posts/{id}/vote` | Member | Toggle vote |

### Comments

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/posts/{id}/comments` | No | List comments (threaded) |
| POST | `/posts/{id}/comments` | Member | Add comment |
| DELETE | `/comments/{id}` | Admin/Creator | Delete comment |

### Attachments

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/posts/{id}/attachments` | No | List attachments for a post |
| POST | `/posts/{id}/attachments` | Author/Admin | Upload image (multipart, max 10MB) |
| GET | `/attachments/{id}` | No | Serve/download an image |
| DELETE | `/attachments/{id}` | Author/Admin | Delete an attachment |

### Auth

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/auth/providers` | No | List active OAuth providers |
| GET | `/auth/me` | Member | Current user info |
| GET | `/auth/{provider}/login` | No | Start OAuth flow |
| GET | `/auth/{provider}/callback` | No | OAuth callback |
| GET | `/auth/logout` | No | Logout |

## Query Parameters (Posts)

| Parameter | Type | Description |
|-----------|------|-------------|
| `sort` | string | `newest`, `oldest`, `most_votes`, `least_votes`, `recently_updated` |
| `status` | string | `open`, `planned`, `in_progress`, `done`, `declined` |
| `category` | string | `feedback`, `bug`, `feature`, `question` |
| `q` | string | Search query (title + description) |
| `offset` | number | Pagination offset (default: 0) |
| `limit` | number | Items per page (default: 20, max: 100) |

## Response Format

All responses are JSON.

### Success

```json
{
  "data": { ... },
  "total": 42,
  "offset": 0,
  "limit": 20
}
```

### Error

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Post not found"
  }
}
```

### Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 204 | No content (deleted) |
| 400 | Bad request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not found |
| 422 | Validation error |
| 500 | Internal error |
