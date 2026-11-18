-- Email notification opt-outs (#73). Users are subscribed to a post by
-- authoring or commenting on it; a row here opts them out of all email.
-- A separate table (not an ALTER on users) keeps the migration idempotent:
-- the runner executes every migration on every boot (see 004).
CREATE TABLE IF NOT EXISTS notification_opt_outs (
    user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
