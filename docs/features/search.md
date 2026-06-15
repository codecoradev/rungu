# Search & Sort

## Full-Text Search

Search across post titles and descriptions:

```bash
curl "https://feedback.example.com/api/projects/:slug/posts?q=dark+mode"
```

## Sorting

Sort posts by any of these options:

| Sort | Description |
|------|-------------|
| `newest` | Most recently created (default) |
| `oldest` | Oldest first |
| `most_votes` | Highest vote count |
| `least_votes` | Lowest vote count |
| `recently_updated` | Most recently updated (status change, comment) |

```bash
curl "https://feedback.example.com/api/projects/:slug/posts?sort=most_votes"
```

## Pagination

```bash
# First page, 20 items
curl "https://feedback.example.com/api/projects/:slug/posts?offset=0&limit=20"

# Second page
curl "https://feedback.example.com/api/projects/:slug/posts?offset=20&limit=20"
```

## Combining Filters

All filters can be combined:

```bash
curl "https://feedback.example.com/api/projects/:slug/posts?status=open&category=feature&sort=most_votes&q=api&offset=0&limit=20"
```
