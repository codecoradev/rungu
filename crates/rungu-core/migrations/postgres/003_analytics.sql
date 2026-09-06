-- Privacy-first analytics events (aggregate counters only — no IP, no cookies, no user agent).
-- Data plane for AI-assisted board management (#186): MCP tools + admin REST read these.
CREATE TABLE IF NOT EXISTS analytics_events (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    post_id     TEXT REFERENCES posts(id) ON DELETE CASCADE,
    event_type  TEXT NOT NULL,                       -- board_view | post_view | vote | comment | post_created
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_analytics_project ON analytics_events(project_id, event_type, created_at);
CREATE INDEX IF NOT EXISTS idx_analytics_post ON analytics_events(post_id, event_type);
