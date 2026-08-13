# Webhooks

Rungu can send HTTP POST callbacks (webhooks) to external services when events happen. This enables real-time integrations with Slack, Discord, Zapier, custom automation, and more.

## How It Works

1. You register a webhook URL for a project
2. When an event fires (e.g., new feedback post), Rungu sends an HTTP POST with a JSON payload
3. The payload is signed with HMAC-SHA256 so you can verify authenticity
4. If your server returns non-2xx, Rungu retries with exponential backoff (up to 3 attempts)

## Supported Events

| Event | Fired When | Payload Fields |
|-------|-----------|----------------|
| `post.created` | New feedback post created | `id`, `title`, `project_id`, `status` |
| `post.status_changed` | Post status updated | `id`, `title`, `old_status`, `new_status` |
| `comment.created` | New comment on a post | `id`, `post_id`, `post_title` |
| `*` | All events | Varies |

## Managing Webhooks

All webhook endpoints require admin authentication.

### Create a Webhook

```bash
curl -X POST http://localhost:3000/api/projects/{slug}/webhooks \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://hooks.slack.com/services/...",
    "events": "*"
  }'
```

If you don't provide a `secret`, Rungu auto-generates one. The secret is used to sign payloads — store it securely.

### List Webhooks

```bash
curl http://localhost:3000/api/projects/{slug}/webhooks \
  -H "Authorization: Bearer <token>"
```

### Update a Webhook

```bash
curl -X PUT http://localhost:3000/api/projects/{slug}/webhooks/{id} \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"is_active": false}'
```

### Delete a Webhook

```bash
curl -X DELETE http://localhost:3000/api/projects/{slug}/webhooks/{id} \
  -H "Authorization: Bearer <token>"
```

### View Delivery Log

```bash
curl "http://localhost:3000/api/projects/{slug}/webhooks/{id}/deliveries?limit=20" \
  -H "Authorization: Bearer <token>"
```

## Verifying Payloads

Every webhook delivery includes an `X-Rungu-Signature` header containing the HMAC-SHA256 hex digest of the request body, computed using your webhook secret.

### Verification Example (Node.js)

```javascript
const crypto = require('crypto');

function verifySignature(rawBody, signature, secret) {
  const expected = crypto
    .createHmac('sha256', secret)
    .update(rawBody)
    .digest('hex');
  return crypto.timingSafeEqual(
    Buffer.from(expected),
    Buffer.from(signature)
  );
}
```

### Verification Example (Python)

```python
import hmac
import hashlib

def verify_signature(raw_body: bytes, signature: str, secret: str) -> bool:
    expected = hmac.new(
        secret.encode(),
        raw_body,
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature)
```

## Retry Strategy

| Attempt | Delay | Cumulative |
|---------|-------|------------|
| 1 | Immediate | 0s |
| 2 | 1 second | 1s |
| 3 | 2 seconds | 3s |
| 4 | 4 seconds | 7s |

After all attempts fail, the delivery is marked as failed in the delivery log. You can view failed deliveries via the API.

## Security

- **SSRF Protection**: Rungu blocks webhook URLs pointing to private IP ranges (10.x, 172.16-31.x, 192.168.x), localhost, link-local addresses, and cloud metadata endpoints (169.254.169.254).
- **HMAC Signing**: All payloads are signed. Always verify the signature to prevent spoofing.
- **Admin Only**: Only project admins can create, view, or manage webhooks.
- **Secret Generation**: If no secret is provided, Rungu generates a random UUID v4 secret.
