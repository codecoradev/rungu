# Voting

Users can vote on posts to signal priority. Each user gets one vote per post — toggle on/off.

## How It Works

- Click the vote button → vote is added (count +1)
- Click again → vote is removed (count -1)
- You can see your current vote state on each post

## API

```bash
# Toggle vote (auth required)
curl -X POST -b cookies.txt \
  https://feedback.example.com/api/posts/{post_id}/vote

# Response
{
  "voted": true,
  "vote_count": 42
}
```

## Sorting

Posts can be sorted by vote count:

```
GET /api/projects/:slug/posts?sort=most_votes
```

Available sort options:
- `newest` — most recently created
- `oldest` — oldest first
- `most_votes` — highest vote count
- `least_votes` — lowest vote count
- `recently_updated` — most recently updated (status change, comment, etc.)
