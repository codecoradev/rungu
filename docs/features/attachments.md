# Image Attachments

Rungu supports image attachments on feedback posts — screenshots of bugs, mockups for feature requests, diagrams, etc.

## Supported Formats

| Format | MIME Type | Magic Bytes |
|--------|-----------|-------------|
| PNG | `image/png` | `89 50 4E 47` |
| JPEG | `image/jpeg` | `FF D8 FF` |
| WebP | `image/webp` | `RIFF ... WEBP` |
| GIF | `image/gif` | `47 49 46 38` |

**Maximum file size:** 10 MB per image.

## Security

- **Magic byte verification** — the server verifies the actual file content, not just the `Content-Type` header. A `.exe` renamed to `.png` will be rejected.
- **`X-Content-Type-Options: nosniff`** — served images always include this header to prevent MIME sniffing attacks.
- **Path traversal prevention** — storage keys are validated to prevent `../` attacks.
- **Author/admin only** — only the post creator or an admin can upload or delete attachments.

## Configuration

### Filesystem (default)

```env
STORAGE_DRIVER=fs
RUNGU_STORAGE_DIR=./uploads
```

In Docker:

```env
STORAGE_DRIVER=fs
RUNGU_STORAGE_DIR=/data/uploads
```

The directory is created automatically on first run.

### S3-compatible (future)

```env
STORAGE_DRIVER=s3
# Not yet implemented — planned for v0.3
```

MinIO, Cloudflare R2, and AWS S3 will be supported via the generic S3 API.

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/posts/{id}/attachments` | None | List attachments for a post |
| POST | `/api/posts/{id}/attachments` | Author/Admin | Upload an image (multipart) |
| GET | `/api/attachments/{id}` | None | Download/serve an image |
| DELETE | `/api/attachments/{id}` | Author/Admin | Delete an attachment |

### Upload Example

```bash
curl -X POST https://rungu.example.com/api/posts/abc123/attachments \
  -H "Cookie: session=YOUR_JWT" \
  -F "file=@screenshot.png"
```

Response:

```json
{
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "post_id": "abc123",
    "filename": "screenshot.png",
    "mime": "image/png",
    "size": 245678,
    "url": "/api/attachments/550e8400-e29b-41d4-a716-446655440000",
    "created_by": "user-uuid",
    "created_at": "2026-01-15T10:30:00Z"
  }
}
```

## Frontend

The post detail page includes an `AttachmentGallery` component:

- **Grid view** — thumbnails displayed in a responsive grid.
- **Lightbox** — click any thumbnail to view full-size. Press `Esc` to close.
- **Upload** — click "Add image" to select files (multi-select supported).
- **Delete** — hover over a thumbnail and click the trash icon (author/admin only).
