# MCP Server

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
| `get_stats` | Project stats (total posts, by status, by category) |
| `get_trending` | Top voted posts in last 7 days |

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

## No Auth Required

Since MCP runs as a local subprocess, no authentication is needed. The agent has direct access to the SQLite database.
