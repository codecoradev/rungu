# MCP Server

> 🔴 **Security: MCP acts as an admin, on both transports.**
>
> Over **stdio** the server runs without authentication as a built-in admin user, so use it only as a local subprocess of an AI agent you control (Claude Code, Cursor) on a trusted machine. Over **HTTP** (`POST /mcp`) anyone holding `RUNGU_API_KEY` can call every tool as an admin: keep the key secret and serve it over HTTPS. Neither transport applies the web app's per-user roles. See [Trust Boundary](#trust-boundary--security) below.

Rungu includes a built-in MCP (Model Context Protocol) server that lets AI agents query and manage feedback directly.

## Setup

Add to your MCP configuration:

### Claude Code (`.claude/settings.json`)

```json
{
  "mcpServers": {
    "rungu": {
      "command": "rungu",
      "args": ["mcp", "--db", "/path/to/rungu.db"]
    }
  }
}
```

### Cursor / Windsurf

```json
{
  "mcpServers": {
    "rungu": {
      "command": "rungu",
      "args": ["mcp", "--db", "/path/to/rungu.db"]
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `list_projects` | List all feedback projects |
| `get_project` | Get project detail by slug |
| `list_posts` | List posts with filters (`status`, `category`, `sort`, `q`) and paging (`page`, `limit`, max 100). Same rules and results as `GET /api/projects/{slug}/posts` |
| `get_post` | Get post detail with comments |
| `create_post` | Submit a new feedback post |
| `update_post_status` | Change post status (open → planned → done) |
| `update_post_category` | Change post category (bug, feature, question, feedback) |
| `delete_post` | Delete a post by ID |
| `vote_post` | Toggle vote on a post |
| `search_posts` | Full-text search across posts, newest first (`q` required; `page`, `limit`) |
| `get_roadmap` | Posts grouped by lifecycle status (planned, in_progress, done), most voted first (`limit` per bucket, default 10, max 50) |
| `get_changelog` | Shipped (done) posts, most recently updated first (`since` RFC 3339, `page`, `limit`) |
| `list_comments` | Get comments for a post |
| `add_comment` | Add comment to a post |
| `delete_comment` | Delete a comment by ID |
| `get_stats` | Project stats (total posts, by status, by category) |
| `get_trending` | Posts gaining votes fastest over the last 7 days (`limit`, default 10) |
| `get_analytics` | Event totals + daily trend (views, votes, comments) — privacy-first aggregates |
| `get_top_posts` | Top posts by views with vote/view conversion (AI prioritization) |
| `list_attachments` | List image attachments for a post |
| `delete_attachment` | Delete an attachment by ID |
| `create_project` | Create a new feedback project (admin). Same name and slug rules as the REST API |
| `delete_project` | Delete a project and all its content (admin, irreversible) |
| `list_webhooks` | List webhook subscriptions for a project (admin) |
| `create_webhook` | Create a webhook subscription (admin; secret returned once). Same URL rules as the REST API |
| `delete_webhook` | Delete a webhook subscription by `id` (admin). Optional `slug` also checks the webhook belongs to that project |

## Example Usage

In Claude Code:

```
"Show me all open bug reports with the most votes"
→ calls list_posts(status=open, category=bug, sort=most_votes)

"Create a feature request for dark mode in the my-saas project"
→ calls create_post(project_slug=my-saas, title="Dark mode support", category=feature)

"What's trending this week?"
→ calls get_trending()
```

## Transport

Both transports run the same 26 tools through the same dispatcher. The transport decides who is acting.

| | stdio (`rungu mcp`) | HTTP (`POST /mcp`) |
|---|---|---|
| Runs as | a subprocess of the agent | the Rungu server |
| Auth | none (local trust) | `Authorization: Bearer <RUNGU_API_KEY>` |
| Acts as | built-in `mcp@rungu.local` admin | the `ai-agent` admin user the key has on the REST API |

stdio uses stdin/stdout: only JSON-RPC is written to stdout and logs go to stderr. HTTP is stateless, one JSON-RPC request per call; see [Configuration](../configuration.md) for the client config.

## Errors

A failed call returns a JSON-RPC error. The code says why, and `error.data.kind` repeats it as a string:

| Code | `kind` | Meaning |
|---|---|---|
| `-32602` | `invalid_params` | Missing or invalid parameter, or input that fails validation |
| `-32003` | `forbidden` | The acting identity isn't allowed to do this |
| `-32004` | `not_found` | The project, post, comment or webhook doesn't exist |
| `-32009` | `conflict` | Clashes with existing data, e.g. a project slug that's taken |
| `-32601` | `method_not_found` | Unknown tool name |
| `-32603` | `internal` | Server fault |

The codes mirror the REST statuses (400, 403, 404, 409). The `message` is the same text the REST API returns.

Mutating tools fire the same webhooks and analytics as the web app. When stdin closes, `rungu mcp` waits up to 45 seconds for in-flight webhook deliveries before exiting.

### Attachment files

`delete_post` and `delete_attachment` also delete the attachment files, so the MCP process needs the **same `RUNGU_STORAGE_DIR` as the server** (pass it in the client's `env` block). If the directory can't be used — for example the client starts `rungu mcp` from a read-only working directory and the default `./uploads` can't be created — the server logs a warning and still starts, but attachment files are left on disk.

## Trust Boundary & Security

The stdio server intentionally has **no authentication**, and both transports act as an admin. This is safe only because of a strict trust assumption:

- **The stdio subprocess inherits the privileges of whatever launches it.** Any agent, editor plugin, or script that can spawn `rungu mcp` can read and mutate the entire SQLite database.
- **`POST /mcp` is as powerful as the API key.** A valid `RUNGU_API_KEY` runs every tool as the `ai-agent` admin. Treat the key like a root credential: rotate it by changing the env and restarting, and never put it in a client you don't control.
- **There is no row-level authorization.** `create_post`, `update_post_status`, `update_post_category`, `delete_post`, `vote_post`, `add_comment`, `delete_comment`, and `delete_attachment` execute as a built-in MCP user with full write access. They go through the same domain operations as the REST API, so input validation, attachment-file cleanup, webhooks, and analytics apply identically — but the admin role means ownership checks always pass.

### Safe deployments

✅ Do:
- Run MCP only on a **single-user, trusted workstation** you control.
- Point `--db` at a **copy** of the production database (read replica, snapshot) when the agent only needs read access.
- Audit the prompts you send to the agent — prompt injection from untrusted content (web pages, issues, emails) can instruct the agent to mutate data via MCP.

❌ Don't:
- Expose `rungu mcp` (or the SQLite file) on a shared host, CI runner, or container reachable by other users.
- Serve `/mcp` over plain HTTP, or share `RUNGU_API_KEY` with people who shouldn't have admin.
- Assume the OAuth/role model from the HTTP API applies to MCP calls — it does not.

### Future hardening

Planned guardrails (tracked separately) include:
- An `RUNGU_MCP_READ_ONLY` mode that disables mutating tools (`create_post`, `update_post_status`, `update_post_category`, `delete_post`, `vote_post`, `add_comment`, `delete_comment`, `delete_attachment`).
- An explicit `RUNGU_MCP_ALLOW_WRITES=true` opt-in before mutating tools are registered.
- Scoped capability tokens for multi-tenant or shared-workstation use cases.

Until those land, treat any MCP-enabled environment as equivalent to handing the agent raw database credentials.
