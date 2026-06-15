# REST API

Full REST API for integration with dashboards, widgets, or custom tools.

## Base URL

```
https://your-domain.com/api
```

## Authentication

All mutating endpoints require authentication via session cookie (set by OAuth login).

```
Cookie: session=<JWT token>
```

## Endpoints

### Projects

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/projects` | No | List all projects |
| POST | `/projects` | Admin | Create project |
| GET | `/projects/:slug` | No | Project detail |
| PATCH | `/projects/:slug` | Admin | Update project |
| DELETE | `/projects/:slug` | Admin | Delete project |

### Posts

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/projects/:slug/posts` | No | List posts (sort, filter, search, paginate) |
| POST | `/projects/:slug/posts` | Member | Create post |
| GET | `/posts/:id` | No | Post detail + comments |
| PATCH | `/posts/:id` | Admin | Update status/category |
| DELETE | `/posts/:id` | Admin/Creator | Delete post |

### Votes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/posts/:id/vote` | Member | Toggle vote |

### Comments

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/posts/:id/comments` | No | List comments (threaded) |
| POST | `/posts/:id/comments` | Member | Add comment |
| DELETE | `/comments/:id` | Admin/Creator | Delete comment |

### Auth

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/auth/providers` | No | List active OAuth providers |
| GET | `/auth/me` | Member | Current user info |
| GET | `/auth/:provider/login` | No | Start OAuth flow |
| GET | `/auth/:provider/callback` | No | OAuth callback |
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
