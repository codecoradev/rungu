# Comments & Threads

Users can comment on any post. Comments support threading via replies.

## How It Works

- Add a top-level comment on any post
- Reply to any comment to create a thread
- Delete your own comments (admins can delete any)

## API

```bash
# List comments
curl https://feedback.example.com/api/posts/{post_id}/comments

# Add comment (auth required)
curl -X POST -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{"content": "This would be great!"}' \
  https://feedback.example.com/api/posts/{post_id}/comments

# Reply to comment (set parent_id)
curl -X POST -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{"content": "Agreed!", "parent_id": "comment-123"}' \
  https://feedback.example.com/api/posts/{post_id}/comments

# Delete comment (admin or creator)
curl -X DELETE -b cookies.txt \
  https://feedback.example.com/api/comments/{comment_id}
```

## Response Format

```json
[
  {
    "id": "abc",
    "content": "This would be great!",
    "created_by": "user@example.com",
    "parent_id": null,
    "children": [
      {
        "id": "def",
        "content": "Agreed!",
        "created_by": "other@example.com",
        "parent_id": "abc",
        "children": []
      }
    ]
  }
]
```
