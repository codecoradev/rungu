-- Webhook subscriptions (per-project, per-event-type).
CREATE TABLE IF NOT EXISTS webhooks (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    secret      TEXT NOT NULL DEFAULT '',           -- HMAC-SHA256 signing secret
    events      TEXT NOT NULL DEFAULT '*',          -- comma-separated event types, '*' = all
    is_active   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_webhooks_project ON webhooks(project_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_active ON webhooks(project_id, is_active);

-- Webhook delivery log (for retry + audit).
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id           TEXT PRIMARY KEY,
    webhook_id   TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    payload      TEXT NOT NULL,
    status_code  INTEGER,                            -- NULL = not yet delivered
    success      INTEGER NOT NULL DEFAULT 0,
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    delivered_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_deliveries_webhook ON webhook_deliveries(webhook_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_deliveries_pending ON webhook_deliveries(success, attempts);
