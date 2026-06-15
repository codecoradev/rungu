# Multi-Board

Rungu supports multiple projects/boards. Each project has its own feedback space with independent posts, votes, and comments.

## Creating Boards

```bash
# CLI
rungu project add "My SaaS Product" --slug "my-saas" --description "Customer feedback"
rungu project add "Mobile App" --slug "mobile" --description "App store reviews & feedback"
rungu project add "Documentation" --slug "docs" --description "Docs feedback"
```

## Accessing Boards

Each board has its own slug-based URL:

```
https://feedback.example.com/board/my-saas
https://feedback.example.com/board/mobile
https://feedback.example.com/board/docs
```

## API

```bash
# List all boards
curl https://feedback.example.com/api/projects

# Get specific board
curl https://feedback.example.com/api/projects/my-saas

# Create board (admin only)
curl -X POST -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{"name": "New Project", "description": "Feedback board"}' \
  https://feedback.example.com/api/projects
```

## Isolation

- Posts, votes, and comments are isolated per board
- Users can access all boards (no per-board auth)
- Admins can manage all boards
