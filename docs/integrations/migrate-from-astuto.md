# Migrate from Astuto

[Astuto](https://github.com/astuto/astuto) is a self-hosted feedback board built with Elixir/Phoenix and PostgreSQL. This guide walks you through migrating an existing Astuto deployment to Rungu — including users, boards, posts, comments, and votes.

## Why Migrate to Rungu?

| Concern | Astuto | Rungu |
|---------|--------|-------|
| **Deployment** | Elixir + PostgreSQL + Docker Compose (3+ containers) | Single static binary or single Docker container |
| **Database** | PostgreSQL (required) | SQLite (default) or PostgreSQL (optional) |
| **AI integration** | None | Built-in [MCP server](/integrations/mcp) for Claude Code, Cursor, etc. |
| **Webhooks** | None | Per-project [webhook](#webhooks-new-in-rungu) system with retry + delivery log |
| **Resource footprint** | ~300–500 MB RAM (BEAM VM + Postgres) | ~20–40 MB RAM (single binary) |
| **Backup** | `pg_dump` of the Postgres database | Copy a single `.db` file |

Rungu is a good fit if you want a lighter-weight feedback board that is easy to deploy on a small VPS, integrates with AI tooling, and can notify external services via webhooks.

## Feature Comparison

| Feature | Astuto | Rungu |
|---------|--------|-------|
| Feedback boards | **Boards** | **Projects** (slug-based URLs) |
| Feedback entries | **Posts** (title, description, status) | **Posts** (title, description, status + category) |
| Categories | Not supported | `feedback`, `bug`, `feature`, `question` |
| Statuses | `open`, `planned`, `started`, `done`, `declined` | `open`, `planned`, `in_progress`, `done`, `declined` |
| Voting | Upvote (one per user) | Toggle vote (one per user) |
| Comments | Flat (single level) | **Threaded** (parent/child replies) |
| Attachments | Not supported | Image attachments (max 10 MB) |
| Full-text search | Not supported | Built-in FTS (title + description) |
| OAuth providers | GitHub, Google | GitHub, Google, Keycloak |
| Email/password login | Yes | No (OAuth only — email-keyed identity) |
| Custom status labels | Yes (per board) | No (fixed 5-state lifecycle) |
| Roadmap view | Yes | Yes (via API/MCP: posts grouped by status) |
| Webhooks | No | Yes |
| MCP (AI agents) | No | Yes |
| API | REST | REST + OpenAPI/Swagger UI |

### What Does Not Migrate

- **Email/password accounts.** Rungu authenticates via OAuth only. Users who log in to Astuto with email/password will need to use Google, GitHub, or Keycloak on Rungu. Their feedback history is preserved (linked by email), but they must re-authenticate through an OAuth provider on first login.
- **Custom board-level statuses.** Astuto allows custom status labels per board. Rungu uses a fixed five-state lifecycle. Custom statuses must be mapped to the closest Rungu equivalent (see [Status Mapping](#status-mapping)).
- **Astuto "boards" metadata** like custom blocks or board-level theming. Only the board name, slug, and description are migrated.

## Before You Begin

### Prerequisites

- A running **Astuto** instance with PostgreSQL access.
- A **Rungu** instance (v0.2+) — see [Installation](/getting-started).
- `psql` CLI or any PostgreSQL client (e.g., DBeaver, pgAdmin) connected to the Astuto database.
- A backup of your Astuto database (`pg_dump`).

### Step 0: Back Up Astuto

```bash
# Full backup of the Astuto database
pg_dump -h localhost -U astuto_user -d astuto_db -F c -f astuto_backup.dump
```

Store this backup safely. The migration script is read-only against the Astuto database (it uses `INSERT ... SELECT`), but a backup protects against mistakes.

### Step 1: Install and Initialize Rungu

Install Rungu using PostgreSQL so the migration script can write directly into the same database (or a separate one):

```bash
# Create a Rungu database (can be the same Postgres instance, different DB)
createdb -h localhost -U postgres rungu_db

# Start Rungu once to run migrations and create the schema
DATABASE_URL="postgres://postgres:password@localhost:5432/rungu_db" \
APP_SECRET="$(openssl rand -hex 32)" \
./rungu serve

# Verify tables exist, then stop the server (Ctrl+C)
psql -h localhost -U postgres -d rungu_db -c "\dt"
```

Rungu's migration runner creates all tables on first startup. Confirm you see `users`, `projects`, `posts`, `votes`, `comments`, and `user_identities`.

::: tip
If your Astuto and Rungu databases are on the **same PostgreSQL instance**, you can run the migration script as a single SQL file with cross-database queries (using `astuto_db.public.users`). If they are on different hosts, point the script at the Astuto database by exporting it first, or use [postgres_fdw](https://www.postgresql.org/docs/current/postgres-fdw.html) for a live cross-server pull.
:::

## Database Migration

The migration script below reads from the Astuto schema and inserts into the Rungu schema. It assumes both schemas are in the same PostgreSQL instance (Astuto in `astuto_db`, Rungu in `rungu_db`). Adjust the database name / schema prefix for your setup.

### Astuto Schema Reference

Astuto's PostgreSQL schema (as of v0.1) uses these key tables:

| Astuto table | Key columns | Maps to Rungu |
|--------------|-------------|---------------|
| `users` | `id`, `email`, `name`, `profile_img`, `role` (`user`/`admin`), `created_at` | `users` |
| `boards` | `id`, `slug`, `name`, `description`, `created_at` | `projects` |
| `posts` | `id`, `board_id`, `user_id`, `title`, `description`, `post_status_id`, `created_at`, `updated_at` | `posts` |
| `post_statuses` | `id`, `name`, `board_id` | Status mapping (see below) |
| `comments` | `id`, `post_id`, `user_id`, `body`, `parent_id`, `created_at` | `comments` |
| `votes` / `post_votes` | `user_id`, `post_id`, `created_at` | `votes` |

::: warning
Astuto's exact column names may vary slightly between versions. Run `\d posts` and `\d post_votes` on your Astuto database to verify column names before running the script. The script includes comments noting where to adjust.
:::

### Status Mapping

Astuto uses a `post_statuses` table (each board can define custom statuses). Rungu uses a fixed set. Map by name:

| Astuto status name (typical) | Rungu status |
|------------------------------|--------------|
| `open`, `new`, `inbox` | `open` |
| `planned`, `considering`, `under review` | `planned` |
| `started`, `in progress`, `in_development` | `in_progress` |
| `done`, `completed`, `shipped`, `closed` | `done` |
| `declined`, `rejected`, `duplicate`, `wontfix` | `declined` |

Any unrecognized status defaults to `open`.

### Migration Script

Save this as `migrate_astuto_to_rungu.sql` and review the comments before running:

```sql
-- ============================================================
-- Astuto → Rungu Migration Script (PostgreSQL)
-- ============================================================
-- Prerequisites:
--   1. Rungu schema must already exist in rungu_db (run `rungu serve` once).
--   2. This script reads from astuto_db and writes to rungu_db.
--   3. Run as a PostgreSQL superuser or a user with access to both databases.
--
-- Adjust the astuto_db reference below if your database name differs.
-- ============================================================

BEGIN;

-- ── 1. Users ─────────────────────────────────────────────────
-- Astuto:  users(id, email, name, profile_img, role, created_at)
-- Rungu:   users(id, email, name, avatar_url, role, created_at, last_login)

INSERT INTO rungu_db.public.users (id, email, name, avatar_url, role, created_at, last_login)
SELECT
    'ast_' || u.id::text,                              -- prefix to avoid ID collisions
    LOWER(TRIM(u.email)),                               -- normalized email
    COALESCE(u.name, split_part(u.email, '@', 1)),      -- fallback to email local-part
    COALESCE(u.profile_img, ''),
    CASE
        WHEN u.role = 'admin' THEN 'admin'
        ELSE 'member'
    END,
    u.created_at,
    COALESCE(u.updated_at, u.created_at)                -- last_login fallback
FROM astuto_db.public.users u
ON CONFLICT (email) DO NOTHING;                         -- skip if email already exists in Rungu

-- Create a mapping view for FK resolution (Astuto ID → Rungu ID)
CREATE OR REPLACE TEMP VIEW _user_map AS
SELECT
    u.id AS astuto_user_id,
    r.id AS rungu_user_id
FROM astuto_db.public.users u
JOIN rungu_db.public.users r ON LOWER(TRIM(u.email)) = r.email;

-- ── 2. Boards → Projects ────────────────────────────────────
-- Astuto:  boards(id, slug, name, description, created_at)
-- Rungu:   projects(id, slug, name, description, created_at)

INSERT INTO rungu_db.public.projects (id, slug, name, description, created_at)
SELECT
    'ast_' || b.id::text,                               -- prefix to avoid ID collisions
    b.slug,                                             -- slugs must be unique; check manually
    b.name,
    COALESCE(b.description, ''),
    b.created_at
FROM astuto_db.public.boards b
ON CONFLICT (slug) DO NOTHING;

-- Board ID mapping
CREATE OR REPLACE TEMP VIEW _board_map AS
SELECT
    b.id AS astuto_board_id,
    r.id AS rungu_project_id
FROM astuto_db.public.boards b
JOIN rungu_db.public.projects r
    ON b.slug = r.slug
    AND r.id LIKE 'ast_%';

-- ── 3. Posts ────────────────────────────────────────────────
-- Astuto:  posts(id, board_id, user_id, title, description, post_status_id, created_at, updated_at)
-- Rungu:   posts(id, project_id, title, description, status, category, vote_count,
--                comment_count, created_by, created_at, updated_at)

INSERT INTO rungu_db.public.posts (
    id, project_id, title, description, status, category,
    vote_count, comment_count, created_by, created_at, updated_at
)
SELECT
    'ast_' || p.id::text,
    bm.rungu_project_id,
    p.title,
    COALESCE(p.description, ''),
    -- Status mapping: Astuto post_status → Rungu fixed lifecycle
    COALESCE(
        (SELECT CASE
            WHEN LOWER(ps.name) IN ('open', 'new', 'inbox') THEN 'open'
            WHEN LOWER(ps.name) IN ('planned', 'considering', 'under review', 'under_review') THEN 'planned'
            WHEN LOWER(ps.name) IN ('started', 'in progress', 'in_progress', 'in development', 'in_development') THEN 'in_progress'
            WHEN LOWER(ps.name) IN ('done', 'completed', 'shipped', 'closed') THEN 'done'
            WHEN LOWER(ps.name) IN ('declined', 'rejected', 'duplicate', 'wontfix', "won't fix") THEN 'declined'
            ELSE NULL                                     -- unknown → fallback below
        END
        FROM astuto_db.public.post_statuses ps
        WHERE ps.id = p.post_status_id),
        'open'                                           -- default for NULL/unmapped statuses
    ),
    'feedback',                                          -- Astuto has no categories; default
    0,                                                   -- vote_count: recomputed below
    0,                                                   -- comment_count: recomputed below
    um.rungu_user_id,
    p.created_at,
    COALESCE(p.updated_at, p.created_at)
FROM astuto_db.public.posts p
JOIN _board_map bm ON p.board_id = bm.astuto_board_id
JOIN _user_map  um ON p.user_id  = um.astuto_user_id
ON CONFLICT (id) DO NOTHING;

-- Post ID mapping
CREATE OR REPLACE TEMP VIEW _post_map AS
SELECT
    p.id AS astuto_post_id,
    r.id AS rungu_post_id
FROM astuto_db.public.posts p
JOIN rungu_db.public.posts r
    ON r.id = 'ast_' || p.id::text;

-- ── 4. Votes ────────────────────────────────────────────────
-- Astuto:  post_votes(user_id, post_id, created_at)
-- Rungu:   votes(user_id, post_id, created_at)  — PK(user_id, post_id)

INSERT INTO rungu_db.public.votes (user_id, post_id, created_at)
SELECT
    um.rungu_user_id,
    pm.rungu_post_id,
    v.created_at
FROM astuto_db.public.post_votes v
JOIN _user_map um ON v.user_id = um.astuto_user_id
JOIN _post_map pm ON v.post_id = pm.astuto_post_id
ON CONFLICT (user_id, post_id) DO NOTHING;

-- Recompute vote_count on migrated posts
UPDATE rungu_db.public.posts rp
SET vote_count = sub.cnt
FROM (
    SELECT post_id, COUNT(*) AS cnt
    FROM rungu_db.public.votes
    GROUP BY post_id
) sub
WHERE rp.id = sub.post_id
  AND rp.id LIKE 'ast_%';

-- ── 5. Comments ─────────────────────────────────────────────
-- Astuto:  comments(id, post_id, user_id, body, parent_id, created_at)
-- Rungu:   comments(id, post_id, parent_id, content, created_by, created_at)

-- Insert top-level comments first (parent_id IS NULL)
INSERT INTO rungu_db.public.comments (id, post_id, parent_id, content, created_by, created_at)
SELECT
    'ast_' || c.id::text,
    pm.rungu_post_id,
    NULL,
    c.body,
    um.rungu_user_id,
    c.created_at
FROM astuto_db.public.comments c
JOIN _post_map pm ON c.post_id = pm.astuto_post_id
JOIN _user_map um ON c.user_id = um.astuto_user_id
WHERE c.parent_id IS NULL
ON CONFLICT (id) DO NOTHING;

-- Insert replies (parent_id IS NOT NULL) — parent must already be inserted
INSERT INTO rungu_db.public.comments (id, post_id, parent_id, content, created_by, created_at)
SELECT
    'ast_' || c.id::text,
    pm.rungu_post_id,
    'ast_' || c.parent_id::text,                          -- remap parent reference
    c.body,
    um.rungu_user_id,
    c.created_at
FROM astuto_db.public.comments c
JOIN _post_map pm ON c.post_id = pm.astuto_post_id
JOIN _user_map um ON c.user_id = um.astuto_user_id
WHERE c.parent_id IS NOT NULL
ON CONFLICT (id) DO NOTHING;

-- Recompute comment_count on migrated posts
UPDATE rungu_db.public.posts rp
SET comment_count = sub.cnt
FROM (
    SELECT post_id, COUNT(*) AS cnt
    FROM rungu_db.public.comments
    GROUP BY post_id
) sub
WHERE rp.id = sub.post_id
  AND rp.id LIKE 'ast_%';

-- ── 6. OAuth Identities (optional) ──────────────────────────
-- If you want pre-linked OAuth identities so users don't need to re-auth
-- on first login, map Astuto's OAuth data. Astuto stores provider info
-- in a `user_oauth` or similar table — adjust the query below to match
-- your Astuto version. Uncomment and adapt as needed.

-- INSERT INTO rungu_db.public.user_identities (id, user_id, provider, provider_id, created_at)
-- SELECT
--     'ast_oid_' || oa.id::text,
--     um.rungu_user_id,
--     CASE LOWER(oa.provider)
--         WHEN 'github' THEN 'github'
--         WHEN 'google' THEN 'google'
--         ELSE 'keycloak'
--     END,
--     oa.provider_uid::text,
--     oa.created_at
-- FROM astuto_db.public.user_oauth oa
-- JOIN _user_map um ON oa.user_id = um.astuto_user_id
-- ON CONFLICT (provider, provider_id) DO NOTHING;

-- ── Done ────────────────────────────────────────────────────

-- Clean up temp views
DROP VIEW IF EXISTS _user_map;
DROP VIEW IF EXISTS _board_map;
DROP VIEW IF EXISTS _post_map;

COMMIT;

-- Verification queries (run after COMMIT):
-- SELECT 'users' AS table, COUNT(*) FROM rungu_db.public.users WHERE id LIKE 'ast_%'
-- UNION ALL SELECT 'projects', COUNT(*) FROM rungu_db.public.projects WHERE id LIKE 'ast_%'
-- UNION ALL SELECT 'posts', COUNT(*) FROM rungu_db.public.posts WHERE id LIKE 'ast_%'
-- UNION ALL SELECT 'votes', COUNT(*) FROM rungu_db.public.votes WHERE post_id LIKE 'ast_%'
-- UNION ALL SELECT 'comments', COUNT(*) FROM rungu_db.public.comments WHERE id LIKE 'ast_%';
```

### Running the Script

```bash
# Option A: Same PostgreSQL instance, both databases present
psql -h localhost -U postgres -d rungu_db -f migrate_astuto_to_rungu.sql

# Option B: If databases are on different servers, use postgres_fdw
# See: https://www.postgresql.org/docs/current/postgres-fdw.html
```

::: warning
**ID prefixing:** The script prefixes all migrated IDs with `ast_` (e.g., `ast_42`). This prevents collisions with any existing Rungu data and makes it easy to identify migrated records. If you are migrating into a **fresh** Rungu database and prefer to keep original numeric IDs, remove the `ast_` prefix from the script — but ensure the ID types match (Astuto uses `INTEGER`, Rungu uses `TEXT`).
:::

### Verifying the Migration

After running the script, verify the row counts:

```sql
-- Should match Astuto counts
SELECT
    (SELECT COUNT(*) FROM users WHERE id LIKE 'ast_%')       AS users_migrated,
    (SELECT COUNT(*) FROM projects WHERE id LIKE 'ast_%')    AS projects_migrated,
    (SELECT COUNT(*) FROM posts WHERE id LIKE 'ast_%')       AS posts_migrated,
    (SELECT COUNT(*) FROM votes WHERE post_id LIKE 'ast_%')  AS votes_migrated,
    (SELECT COUNT(*) FROM comments WHERE id LIKE 'ast_%')    AS comments_migrated;
```

Then spot-check via the Rungu UI or API:

```bash
# List migrated projects
curl http://localhost:3000/api/projects

# Check a specific project's posts
curl http://localhost:3000/api/projects/{slug}/posts?sort=most_votes
```

## Configuration Migration

Astuto is configured via environment variables (typically in a `.env` file for Docker Compose). Map them to Rungu's equivalents:

| Astuto ENV | Rungu ENV | Notes |
|------------|-----------|-------|
| `BASE_URL` | `APP_URL` | Base URL for the app + OAuth redirect URIs |
| `SECRET_KEY` / `SECRET_KEY_BASE` | `APP_SECRET` | JWT signing secret. **Generate a new one** — do not reuse Astuto's. Run `openssl rand -hex 32`. |
| `DATABASE_URL` | `DATABASE_URL` | Same format: `postgres://user:pass@host:port/db`. Or omit and use `RUNGU_DB` for SQLite. |
| `ENABLE_EMAIL_LOGIN` | _(no equivalent)_ | Rungu is OAuth-only. Email/password login is not supported. |
| `GITHUB_CLIENT_ID` | `GITHUB_CLIENT_ID` | Same — reuse your existing GitHub OAuth App |
| `GITHUB_CLIENT_SECRET` | `GITHUB_CLIENT_SECRET` | Same |
| `GOOGLE_CLIENT_ID` | `GOOGLE_CLIENT_ID` | Same — reuse your existing Google OAuth credentials |
| `GOOGLE_CLIENT_SECRET` | `GOOGLE_CLIENT_SECRET` | Same |
| _(none)_ | `ADMIN_EMAILS` | Comma-separated admin email allowlist. In Astuto, admins are set in the DB. In Rungu, set via `ADMIN_EMAILS` env var. |
| `SMTP_*` | _(no equivalent)_ | Rungu does not send email. Remove SMTP config. |
| `PORT` | `RUNGU_LISTEN` | Astuto uses `PORT=4000`. Rungu uses `RUNGU_LISTEN=0.0.0.0:3000` (address + port). |

### Example: Converted `.env`

**Astuto `.env`:**

```env
BASE_URL=https://feedback.example.com
SECRET_KEY_BASE=long_random_string
DATABASE_URL=postgres://astuto:pass@db:5432/astuto
PORT=4000
ENABLE_EMAIL_LOGIN=true
GITHUB_CLIENT_ID=Iv1.abc123
GITHUB_CLIENT_SECRET=def456
GOOGLE_CLIENT_ID=xyz.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=GOCSPX-abc
SMTP_SERVER=smtp.example.com
SMTP_PORT=587
SMTP_USERNAME=noreply@example.com
SMTP_PASSWORD=secret
```

**Rungu `.env` (equivalent):**

```env
# Server
RUNGU_LISTEN=0.0.0.0:3000
APP_URL=https://feedback.example.com
APP_SECRET=$(openssl rand -hex 32)    # generate a NEW secret, do not reuse

# Database — can keep the same Postgres instance
DATABASE_URL=postgres://rungu:pass@localhost:5432/rungu_db

# Admin emails (list the emails that had admin role in Astuto)
ADMIN_EMAILS=admin@example.com,owner@example.com

# OAuth — reuse the same credentials
GITHUB_CLIENT_ID=Iv1.abc123
GITHUB_CLIENT_SECRET=def456
GOOGLE_CLIENT_ID=xyz.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=GOCSPX-abc

# NOTE: SMTP_* removed — Rungu does not send email.
# NOTE: ENABLE_EMAIL_LOGIN removed — Rungu is OAuth-only.
```

::: tip OAuth Redirect URIs
When you switch domains or ports, update the **redirect URIs** in your OAuth provider consoles:

- **GitHub:** Settings → Developer settings → OAuth Apps → your app → Authorization callback URL
- **Google:** Cloud Console → APIs & Credentials → your credential → Authorized redirect URIs

Set them to: `{APP_URL}/auth/github/callback` and `{APP_URL}/auth/google/callback`
:::

### Admin Users

In Astuto, the `admin` role is stored directly in the `users` table. The migration script maps Astuto `admin` users to Rungu `admin` role. However, Rungu also re-evaluates the role on every login based on `ADMIN_EMAILS`. To ensure admins keep their privileges:

```bash
# After migration, set ADMIN_EMAILS to include all admin email addresses
export ADMIN_EMAILS=admin1@example.com,admin2@example.com

# Verify by querying the migrated admin users
psql -d rungu_db -c "SELECT email, name FROM users WHERE role = 'admin' AND id LIKE 'ast_%';"
```

## Webhooks (New in Rungu)

Astuto does not have a webhook system. Rungu's webhook feature lets you receive HTTP notifications when events happen (new post, status change, new comment, new vote).

After migration, set up webhooks to integrate with Slack, Discord, Zapier, or your own automation:

```bash
# Create a webhook for a project (admin only)
curl -X POST -b cookies.txt \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://hooks.slack.com/services/xxx/yyy/zzz",
    "events": "post.created,post.status_changed",
    "secret": "my_webhook_secret"
  }' \
  http://localhost:3000/api/projects/{slug}/webhooks
```

See the [REST API docs](/integrations/api) for the full webhook endpoint reference.

## Post-Migration Checklist

- [ ] **Verify row counts** — confirm users, projects, posts, votes, and comments match between Astuto and Rungu.
- [ ] **Test OAuth login** — log in via GitHub and Google. Confirm your existing email address links to the migrated account (check that your posts/comments appear under your name).
- [ ] **Verify admin access** — confirm admin users can see admin-only UI (status/category management). Check `ADMIN_EMAILS` is set.
- [ ] **Check board URLs** — visit each migrated project at `/board/{slug}`. Astuto and Rungu slugs should match, but verify.
- [ ] **Test voting** — vote on a migrated post. Confirm the vote count increments and the post re-sorts.
- [ ] **Test comments** — add a comment and a threaded reply on a migrated post.
- [ ] **Verify status mapping** — review posts that had custom Astuto statuses. Confirm they landed in the correct Rungu status.
- [ ] **Set up webhooks** — configure webhooks for external notifications (Slack, Discord, etc.).
- [ ] **Update DNS / reverse proxy** — point your domain to the Rungu instance. Update OAuth redirect URIs in GitHub/Google console.
- [ ] **Remove Astuto** — once verified, shut down the Astuto containers. Keep the `pg_dump` backup for 30+ days as a safety net.
- [ ] **Set up backups** — configure regular backups of the Rungu database (`pg_dump` for PostgreSQL, or file copy for SQLite).
- [ ] **Notify users** — inform your community about the migration, new login flow (OAuth only), and any URL changes.

## Troubleshooting

### Duplicate email errors

If you see `ON CONFLICT (email) DO NOTHING` skipping rows, it means a user with that email already exists in Rungu (perhaps from a test login). This is expected — the user is already there. Verify the count difference is accounted for.

### Missing posts (FK violation)

Posts require a valid `project_id` and `created_by`. If a post references a board or user that was skipped (e.g., a deleted user), the `JOIN` in the migration will drop that post. Check the Astuto database for orphaned records:

```sql
-- Find posts by deleted users
SELECT p.* FROM astuto_db.public.posts p
LEFT JOIN astuto_db.public.users u ON p.user_id = u.id
WHERE u.id IS NULL;
```

### Slug conflicts

If two Astuto boards have the same slug (shouldn't happen — slugs are unique in Astuto), or a slug conflicts with an existing Rungu project, the second board will be skipped. Rename the slug in the script or the Rungu project before re-running.

### Status mapping misses

Posts with an unrecognized status default to `open`. To find them:

```sql
SELECT rp.title, ps.name AS astuto_status
FROM rungu_db.public.posts rp
JOIN astuto_db.public.posts ap ON rp.id = 'ast_' || ap.id::text
JOIN astuto_db.public.post_statuses ps ON ap.post_status_id = ps.id
WHERE rp.status = 'open'
  AND LOWER(ps.name) NOT IN ('open', 'new', 'inbox');
```

Update them manually via the Rungu API or UI.

## Getting Help

- [GitHub Issues](https://github.com/codecoradev/rungu/issues) — report migration problems
- [REST API docs](/integrations/api) — for post-migration data fixes via API
- [Configuration](/configuration) — full environment variable reference
- [CLI Reference](/cli-reference) — all `rungu` commands
