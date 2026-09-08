-- Official team response (#205): admins pin one comment as the post's
-- canonical team reply. A separate table (not an ALTER on posts) keeps the
-- migration idempotent — the runner executes every migration on every boot,
-- so 004 must be as re-runnable as 001–003 (IF NOT EXISTS).
CREATE TABLE IF NOT EXISTS post_official_response (
    post_id    TEXT PRIMARY KEY REFERENCES posts(id) ON DELETE CASCADE,
    comment_id TEXT REFERENCES comments(id) ON DELETE CASCADE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
