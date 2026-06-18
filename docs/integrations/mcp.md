# MCP Server

> ⚠️ **Experimental** — tool handlers currently return stub data. Full implementation tracked in [#28](https://github.com/codecoradev/rungu/issues/28).

> 🔴 **Security: local trusted environments only.**
>
> The MCP server runs **without authentication** and calls the storage layer directly. It is designed exclusively for use as a local subprocess of an AI agent you control (e.g. Claude Code, Cursor) on a trusted machine. **Never expose the MCP server over the network or to a shared/multi-user environment** — any process that can reach the database file (or invoke the subprocess) can read and write all feedback data, impersonate any user, and bypass every application-level authorization check. See [Trust Boundary](#trust-boundary--security) below.

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
| `list_posts` | List posts with filters (status, category, sort) |
| `get_post` | Get post detail with comments |
| `create_post` | Submit a new feedback post |
| `update_post_status` | Change post status (open → planned → done) |
| `vote_post` | Toggle vote on a post |
| `search_posts` | Full-text search across posts |
| `list_comments` | Get comments for a post |
| `add_comment` | Add comment to a post |
| `delete_comment` | Delete a comment by ID |
| `get_stats` | Project stats (total posts, by status, by category) |
| `get_trending` | Top voted posts in last 7 days |
| `list_attachments` | List image attachments for a post |

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

The MCP server uses **stdio** transport (stdin/stdout). No HTTP server needed — it runs as a subprocess of the AI agent.

## Trust Boundary & Security

The MCP server intentionally has **no authentication**. This is safe only because of a strict trust assumption:

- **The MCP subprocess inherits the privileges of whatever launches it.** Any agent, editor plugin, or script that can spawn `rungu mcp` can read and mutate the entire SQLite database.
- **There is no row-level authorization.** `update_post_status`, `create_post`, `vote_post`, and `add_comment` execute as a built-in MCP user with full write access.

### Safe deployments

✅ Do:
- Run MCP only on a **single-user, trusted workstation** you control.
- Point `--db` at a **copy** of the production database (read replica, snapshot) when the agent only needs read access.
- Audit the prompts you send to the agent — prompt injection from untrusted content (web pages, issues, emails) can instruct the agent to mutate data via MCP.

❌ Don't:
- Expose the MCP server (or the SQLite file) on a shared host, CI runner, or container reachable by other users.
- Wire MCP into a production deployment alongside the HTTP server.
- Assume the OAuth/role model from the HTTP API applies to MCP calls — it does not.

### Future hardening

Planned guardrails (tracked separately) include:
- An `RUNGU_MCP_READ_ONLY` mode that disables mutating tools (`create_post`, `update_post_status`, `vote_post`, `add_comment`).
- An explicit `RUNGU_MCP_ALLOW_WRITES=true` opt-in before mutating tools are registered.
- Scoped capability tokens for multi-tenant or shared-workstation use cases.

Until those land, treat any MCP-enabled environment as equivalent to handing the agent raw database credentials.
