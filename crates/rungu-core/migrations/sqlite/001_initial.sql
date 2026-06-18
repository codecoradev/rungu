-- Initial schema for Rungu feedback board.
-- Run by rungu_core::run_migrations() on first startup.

-- Users (identity = email)
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL DEFAULT '',
    avatar_url  TEXT NOT NULL DEFAULT '',
    role        TEXT NOT NULL DEFAULT 'member',  -- member | admin
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_login  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- OAuth identity links (one user can have multiple providers)
CREATE TABLE IF NOT EXISTS user_identities (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,                   -- google | github | keycloak
    provider_id TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, provider_id)
);
CREATE INDEX IF NOT EXISTS idx_user_identities_user ON user_identities(user_id);

-- Projects
CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_projects_slug ON projects(slug);

-- Posts (feedback/bug/feature/question)
CREATE TABLE IF NOT EXISTS posts (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title         TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    status        TEXT NOT NULL DEFAULT 'open',        -- open | planned | in_progress | done | declined
    category      TEXT NOT NULL DEFAULT 'feedback',    -- feedback | bug | feature | question
    vote_count    INTEGER NOT NULL DEFAULT 0,
    comment_count INTEGER NOT NULL DEFAULT 0,
    created_by    TEXT NOT NULL REFERENCES users(id),
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_posts_project ON posts(project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_posts_votes ON posts(project_id, vote_count DESC);
CREATE INDEX IF NOT EXISTS idx_posts_status ON posts(project_id, status);
CREATE INDEX IF NOT EXISTS idx_posts_category ON posts(project_id, category);

-- Votes (toggle: 1 user = 1 vote per post)
CREATE TABLE IF NOT EXISTS votes (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id     TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, post_id)
);

-- Comments (threaded)
CREATE TABLE IF NOT EXISTS comments (
    id          TEXT PRIMARY KEY,
    post_id     TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    parent_id   TEXT REFERENCES comments(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    created_by  TEXT NOT NULL REFERENCES users(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_comments_post ON comments(post_id, created_at);

-- Full-text search (FTS5)
CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
    title,
    description,
    content='posts',
    content_rowid='rowid'
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER IF NOT EXISTS posts_ai AFTER INSERT ON posts BEGIN
    INSERT INTO posts_fts(rowid, title, description)
    VALUES (new.rowid, new.title, new.description);
END;

CREATE TRIGGER IF NOT EXISTS posts_ad AFTER DELETE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, title, description)
    VALUES ('delete', old.rowid, old.title, old.description);
END;

CREATE TRIGGER IF NOT EXISTS posts_au AFTER UPDATE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, title, description)
    VALUES ('delete', old.rowid, old.title, old.description);
    INSERT INTO posts_fts(rowid, title, description)
    VALUES (new.rowid, new.title, new.description);
END;

-- Post attachments (images)
CREATE TABLE IF NOT EXISTS post_attachments (
    id           TEXT PRIMARY KEY,
    post_id      TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    mime         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    created_by   TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_attachments_post ON post_attachments(post_id);
