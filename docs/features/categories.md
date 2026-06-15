# Categories & Status

## Categories

Each post has a category to help organize feedback:

| Category | Description |
|----------|-------------|
| `feedback` | General feedback or suggestion (default) |
| `bug` | Bug report or issue |
| `feature` | Feature request |
| `question` | Question about the product |

## Status

Posts move through a status lifecycle:

| Status | Description |
|--------|-------------|
| `open` | New, unreviewed (default) |
| `planned` | Accepted, planned for future |
| `in_progress` | Currently being worked on |
| `done` | Completed and shipped |
| `declined` | Not planned |

## API

```bash
# Filter by status
curl https://feedback.example.com/api/projects/:slug/posts?status=open

# Filter by category
curl https://feedback.example.com/api/projects/:slug/posts?category=bug

# Combine filters
curl "https://feedback.example.com/api/projects/:slug/posts?status=open&category=feature"

# Update status (admin only)
curl -X PATCH -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{"status": "planned"}' \
  https://feedback.example.com/api/posts/{post_id}
```
