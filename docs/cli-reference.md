# CLI Reference

## `rungu serve`

Start the HTTP server.

```bash
rungu serve [options]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--listen` | `0.0.0.0:3000` | HTTP listen address |
| `--db` | `rungu.db` | SQLite database path |

### Example

```bash
rungu serve --db /data/feedback.db --listen 0.0.0.0:8080
```

## `rungu project add`

Create a new feedback project/board.

```bash
rungu project add [options] <name>
```

| Option | Default | Description |
|--------|---------|-------------|
| `--slug` | auto (from name) | URL slug for the project |
| `--description` | `""` | Project description |

### Example

```bash
rungu project add "My SaaS" --slug "my-saas" --description "Customer feedback for My SaaS"
```

## `rungu project list`

List all projects.

```bash
rungu project list
```

## `rungu healthcheck`

Exit 0 if healthy (useful for Docker HEALTHCHECK).

```bash
rungu healthcheck --db feedback.db
echo $?  # 0 = healthy
```

## `rungu mcp`

Start MCP server over stdio. Used by AI agents (Claude Code, Cursor, etc.).

```bash
rungu mcp --db feedback.db
```

### MCP Configuration (Claude Code)

```json
{
  "mcpServers": {
    "rungu": {
      "command": "rungu",
      "args": ["mcp", "--db", "/path/to/feedback.db"]
    }
  }
}
```

## Global Options

| Option | Default | Description |
|--------|---------|-------------|
| `--db` | `rungu.db` | SQLite database path |
| `--log-level` | `info` | Log level (trace, debug, info, warn, error) |
| `-h, --help` | — | Show help |
| `-V, --version` | — | Show version |
