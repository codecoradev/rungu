//! # store
//!
//! SQLite storage operations for Rungu.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rungu_proto::*;
use sqlx::{AnyPool, Row, any::AnyRow};

/// Parse a timestamp string from the DB (with fallback).
fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Parse a just-generated RFC3339 timestamp (infallible for our own `to_rfc3339()`).
fn parse_now(s: &str) -> DateTime<Utc> {
    // now_ts() always produces valid RFC3339, so this is truly infallible.
    DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now())
}

/// Parse UserRole from SQLite TEXT column.
fn parse_role(s: &str) -> UserRole {
    match s {
        "admin" => UserRole::Admin,
        _ => UserRole::Member,
    }
}

/// Map a SQLite row to a Project.
fn map_project(row: &AnyRow) -> Project {
    Project {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        description: row.get("description"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
    }
}

/// Map a SQLite row to a User.
fn map_user(row: &AnyRow) -> User {
    User {
        id: row.get("id"),
        email: row.get("email"),
        name: row.get("name"),
        avatar_url: row.get("avatar_url"),
        role: parse_role(row.get::<&str, _>("role")),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
        last_login: parse_ts(row.get::<&str, _>("last_login")),
    }
}

/// Map a SQLite row to a PostDetail (with user join + vote status).
fn map_post_detail(row: &AnyRow) -> PostDetail {
    let post = map_post(row);
    let creator = UserSummary {
        id: row.get("user_id"),
        email: row.get("user_email"),
        name: row.get("user_name"),
        avatar_url: row.get("user_avatar"),
    };
    PostDetail { post, creator, user_voted: false }
}

/// Map a SQLite row to a Post.
fn map_post(row: &AnyRow) -> Post {
    Post {
        id: row.get("id"),
        project_id: row.get("project_id"),
        title: row.get("title"),
        description: row.get("description"),
        status: parse_status(row.get::<&str, _>("status")),
        category: parse_category(row.get::<&str, _>("category")),
        vote_count: row.get("vote_count"),
        comment_count: row.get("comment_count"),
        created_by: row.get("created_by"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
        updated_at: parse_ts(row.get::<&str, _>("updated_at")),
    }
}

/// Map a SQLite row to a Comment.
fn map_comment(row: &AnyRow) -> Comment {
    Comment {
        id: row.get("id"),
        post_id: row.get("post_id"),
        parent_id: row.get("parent_id"),
        content: row.get("content"),
        created_by: row.get("created_by"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
    }
}

/// Map a SQLite row to a CommentDetail (with user join).
fn map_comment_detail(row: &AnyRow) -> CommentDetail {
    let comment = map_comment(row);
    let creator = UserSummary {
        id: row.get("user_id"),
        email: row.get("user_email"),
        name: row.get("user_name"),
        avatar_url: row.get("user_avatar"),
    };
    CommentDetail { comment, creator }
}

/// Convert PostStatus to DB string (avoid format! Debug for underscore variants).
fn status_to_str(s: PostStatus) -> &'static str {
    match s {
        PostStatus::Open => "open",
        PostStatus::Planned => "planned",
        PostStatus::InProgress => "in_progress",
        PostStatus::Done => "done",
        PostStatus::Declined => "declined",
    }
}

/// Convert PostCategory to DB string.
fn category_to_str(c: PostCategory) -> &'static str {
    match c {
        PostCategory::Feedback => "feedback",
        PostCategory::Bug => "bug",
        PostCategory::Feature => "feature",
        PostCategory::Question => "question",
    }
}

/// Sanitize a user-supplied search term into a safe FTS5 MATCH expression.
///
/// FTS5 query syntax ("AND", "OR", "NEAR", `*`, `^`, `"..."`, column filters)
/// would let a crafted query do expensive work or syntax-error out. To keep
/// behavior predictable and safe, we split the input on whitespace and turn
/// each token into a prefix-match token (`token*`), then join with implicit
/// AND (space-separated). Empty/whitespace input yields an empty string,
/// which the caller treats as "no MATCH".
///
/// Example: `"dark mode"` → `"dark* mode*"`.
fn sanitize_fts_query(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split_whitespace()
        // Drop anything that's only punctuation — avoids feeding FTS5 lone
        // operators that would either error or match nothing.
        .filter(|tok| tok.chars().any(|c| c.is_alphanumeric()))
        .map(|tok| {
            // Strip any FTS5-special chars so the user can't inject query syntax.
            // We keep alphanumerics, underscores, and hyphens (common in
            // identifiers like "v0-1-2"); everything else is dropped per-token.
            let cleaned: String = tok.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            if cleaned.is_empty() { cleaned } else { format!("{cleaned}*") }
        })
        .filter(|tok| !tok.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Which full-text search path `list_posts` should use, if any.
///
/// Kept as a small enum (rather than two `Option`s) so the JOIN, WHERE
/// fragment, and bind value all branch on a single decision.
enum Search {
    /// SQLite FTS5: bind a sanitized prefix-token MATCH expression.
    Fts5(String),
    /// PostgreSQL: bind the raw query to `plainto_tsquery` against the
    /// generated `search_tsv` column.
    PgTsv(String),
    /// No search filter applied.
    None,
}

/// Parse PostStatus from SQLite TEXT column.
fn parse_status(s: &str) -> PostStatus {
    match s {
        "planned" => PostStatus::Planned,
        "in_progress" => PostStatus::InProgress,
        "done" => PostStatus::Done,
        "declined" => PostStatus::Declined,
        _ => PostStatus::Open,
    }
}

/// Parse PostCategory from SQLite TEXT column.
fn parse_category(s: &str) -> PostCategory {
    match s {
        "bug" => PostCategory::Bug,
        "feature" => PostCategory::Feature,
        "question" => PostCategory::Question,
        _ => PostCategory::Feedback,
    }
}

/// Storage layer — all database operations.
#[derive(Clone)]
pub struct Store {
    pool: AnyPool,
    /// True when the underlying database is SQLite. Drives FTS5 vs PostgreSQL
    /// `tsvector` search-path selection (SQLite has the `posts_fts` virtual
    /// table populated by triggers; PostgreSQL uses a generated `tsvector`
    /// column + GIN index).
    is_sqlite: bool,
    /// Short-TTL cache for `get_project_by_slug`. Project slugs are resolved
    /// on nearly every board/vote/comment request, but rarely change, so a
    /// small cache eliminates most of those round-trips. Mutations to projects
    /// (`create`/`update`/`delete`) invalidate the whole cache — projects are
    /// few and the simplicity beats per-key tracking.
    project_cache: moka::future::Cache<String, Project>,
}

/// How long a cached `Project` row stays fresh before re-hitting the DB.
const PROJECT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// Upper bound on cached projects. Projects are few in practice; this just
/// caps memory if a caller enumerates many slugs (including misses).
const PROJECT_CACHE_MAX_ENTRIES: u64 = 256;

impl Store {
    pub fn new(pool: AnyPool) -> Self {
        Self::build(pool, false)
    }

    /// Construct a Store with explicit backend knowledge.
    ///
    /// `is_sqlite` should be `true` when the pool was opened against a
    /// `sqlite:` connection. `lib::open_pool` already detects this when the
    /// URL starts with `sqlite:` — callers should pass that detection result
    /// here so the store can pick the right query dialect.
    pub fn new_with_kind(pool: AnyPool, is_sqlite: bool) -> Self {
        Self::build(pool, is_sqlite)
    }

    fn build(pool: AnyPool, is_sqlite: bool) -> Self {
        let project_cache = moka::future::Cache::builder()
            .time_to_live(PROJECT_CACHE_TTL)
            .max_capacity(PROJECT_CACHE_MAX_ENTRIES)
            .build();
        Self { pool, is_sqlite, project_cache }
    }

    /// Get a reference to the pool.
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    // ── Projects ────────────────────────────────────────────────────

    /// List all projects.
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query("SELECT id, slug, name, description, created_at FROM projects ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .context("Failed to list projects")?;
        Ok(rows.iter().map(map_project).collect())
    }

    /// Get a project by slug.
    ///
    /// Serves from a short-TTL in-memory cache; misses fall through to the DB
    /// and populate the cache. Negative results (slug not found) are **not**
    /// cached — a subsequent create with the same slug should be visible
    /// immediately.
    pub async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        if let Some(cached) = self.project_cache.get(slug).await {
            return Ok(Some(cached));
        }
        let row = sqlx::query("SELECT id, slug, name, description, created_at FROM projects WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get project")?;
        let project = row.as_ref().map(map_project);
        if let Some(ref p) = project {
            self.project_cache.insert(slug.to_string(), p.clone()).await;
        }
        Ok(project)
    }

    /// Get a project by ID.
    pub async fn get_project_by_id(&self, id: &str) -> Result<Option<Project>> {
        let row = sqlx::query("SELECT id, slug, name, description, created_at FROM projects WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get project")?;
        Ok(row.as_ref().map(map_project))
    }

    /// Create a new project.
    pub async fn create_project(&self, name: &str, slug: &str, description: &str) -> Result<Project> {
        let id = super::new_id();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO projects (id, slug, name, description, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to create project")?;

        // Defensive cache invalidation: a negative (miss) result is never
        // cached, so this is strictly unnecessary today — but keep the cache
        // correct regardless of future caching-policy changes.
        self.project_cache.invalidate_all();

        Ok(Project {
            id,
            slug: slug.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            created_at: parse_now(&now),
        })
    }

    /// Update a project's name and/or description.
    pub async fn update_project(
        &self,
        project_id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Project> {
        if name.is_none() && description.is_none() {
            anyhow::bail!("At least one of name or description must be provided");
        }

        if let Some(n) = name {
            let n = n.trim();
            if n.is_empty() {
                anyhow::bail!("Project name cannot be empty");
            }
            sqlx::query("UPDATE projects SET name = ? WHERE id = ?")
                .bind(n)
                .bind(project_id)
                .execute(&self.pool)
                .await
                .context("Failed to update project name")?;
        }

        if let Some(d) = description {
            sqlx::query("UPDATE projects SET description = ? WHERE id = ?")
                .bind(d)
                .bind(project_id)
                .execute(&self.pool)
                .await
                .context("Failed to update project description")?;
        }

        // Invalidate the project cache — name/description changed. We clear
        // the whole cache rather than one key because `update_project` is
        // called by id (the cached key is the slug).
        self.project_cache.invalidate_all();

        // Fetch and return the updated row
        self.get_project_by_id(project_id).await?.context("Project disappeared after update")
    }

    /// Delete a project by ID.
    ///
    /// Cascading deletes (posts, votes, comments) are handled by FK constraints.
    pub async fn delete_project(&self, project_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(project_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete project")?;
        // Invalidate the project cache so a re-create with the same slug is
        // visible immediately and a stale row isn't served post-delete.
        self.project_cache.invalidate_all();
        Ok(())
    }

    // ── Posts ───────────────────────────────────────────────────────────

    /// List posts for a project with filters and sorting.
    ///
    /// Uses positional `?` parameter binding — user input is NEVER interpolated into SQL.
    /// The search query uses `LIKE ?` with the pattern passed as a bind parameter.
    pub async fn list_posts(&self, params: ListPostsParams<'_>) -> Result<(Vec<PostDetail>, i64)> {
        // Search path selection:
        // - SQLite: FTS5 virtual table `posts_fts` (maintained by triggers) via
        //   `posts_fts MATCH ?`. Faster and relevance-ranked. User input is
        //   sanitized into prefix tokens so FTS5 query syntax can't error out
        //   or do expensive work.
        // - PostgreSQL: generated `search_tsv` tsvector column + GIN index via
        //   `p.search_tsv @@ plainto_tsquery(?)`. `plainto_tsquery` already
        //   rejects unsafe syntax, so we bind the raw (trimmed) query.
        // - Empty / punctuation-only input → no search filter.
        let search = match (self.is_sqlite, params.query) {
            (true, Some(q)) => {
                let sanitized = sanitize_fts_query(q);
                if sanitized.is_empty() { Search::None } else { Search::Fts5(sanitized) }
            }
            (false, Some(q)) => {
                let trimmed = q.trim();
                if trimmed.is_empty() { Search::None } else { Search::PgTsv(trimmed.to_string()) }
            }
            _ => Search::None,
        };

        // Build WHERE clause fragments — only add conditions for filters that are present.
        // Each `?` is a positional placeholder bound later via .bind().
        let mut conditions = vec!["p.project_id = ?".to_string()];

        if params.status.is_some() {
            conditions.push("p.status = ?".to_string());
        }
        if params.category.is_some() {
            conditions.push("p.category = ?".to_string());
        }
        if params.since.is_some() {
            // Incremental-pull lower bound on `updated_at` (used by changelog).
            conditions.push("p.updated_at >= ?".to_string());
        }

        let where_sql = conditions.join(" AND ");

        // Sort — safe because it's a hardcoded match, not user input
        let order = match params.sort {
            // FTS5 with most-relevant ranking surfaces in `?sort=newest` only if no
            // explicit vote/order preference; preserve existing behavior otherwise.
            PostSort::Newest => "p.created_at DESC",
            PostSort::Oldest => "p.created_at ASC",
            PostSort::MostVotes => "p.vote_count DESC, p.created_at DESC",
            PostSort::LeastVotes => "p.vote_count ASC, p.created_at DESC",
            PostSort::RecentlyUpdated => "p.updated_at DESC",
        };

        // Search JOIN + WHERE fragment, emitted only when a search path is active.
        // SQLite joins the FTS5 table; PostgreSQL filters on its generated tsvector.
        let (search_join, search_where): (&str, &str) = match &search {
            Search::Fts5(_) => ("JOIN posts_fts ON posts_fts.rowid = p.rowid", " AND posts_fts MATCH ?"),
            Search::PgTsv(_) => ("", " AND p.search_tsv @@ plainto_tsquery(?)"),
            Search::None => ("", ""),
        };

        // Helper: bind optional filter values in order (project_id is first).
        macro_rules! bind_filters {
            ($query:expr) => {{
                let q = $query.bind(params.project_id);
                let q = if let Some(ref s) = params.status { q.bind(status_to_str(*s)) } else { q };
                let q = if let Some(ref c) = params.category { q.bind(category_to_str(*c)) } else { q };
                // Bind in SQL placeholder order: `since` condition is part of
                // `where_sql` and comes BEFORE the appended `search_where`
                // fragment, so `since` must be bound before the search token.
                let q = if let Some(ts) = params.since {
                    // Bind the `updated_at >= ?` lower bound as an RFC3339 string.
                    q.bind(ts.to_rfc3339())
                } else {
                    q
                };
                match &search {
                    Search::Fts5(t) | Search::PgTsv(t) => q.bind(t.clone()),
                    Search::None => q,
                }
            }};
        }

        // Count query (same WHERE, no LIMIT/OFFSET)
        let count_sql = format!("SELECT COUNT(*) FROM posts p {search_join} WHERE {where_sql}{search_where}");
        let total: i64 = bind_filters!(sqlx::query_scalar::<_, i64>(&count_sql))
            .fetch_one(&self.pool)
            .await
            .context("Failed to count posts")?;

        // Main query with LIMIT/OFFSET appended
        let sql = format!(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM posts p \
             {search_join}
             LEFT JOIN users u ON p.created_by = u.id \
             WHERE {where_sql}{search_where} \
             ORDER BY {order} \
             LIMIT ? OFFSET ?"
        );

        let query = bind_filters!(sqlx::query(&sql)).bind(params.limit).bind(params.offset);

        let rows = query.fetch_all(&self.pool).await.context("Failed to list posts")?;
        let mut posts: Vec<PostDetail> = rows.iter().map(map_post_detail).collect();

        // Batch-populate `user_voted` for the requesting user. Previously this
        // was always `false` in list views (only `get_post` set it per row),
        // which left the board unable to show which posts the current user had
        // voted on. A naive per-row lookup would be N+1; instead we do one
        // indexed query over the `votes` (user_id, post_id) primary key.
        if let Some(uid) = params.user_id {
            if !posts.is_empty() {
                let placeholders = std::iter::repeat_n("?", posts.len()).collect::<Vec<_>>().join(",");
                let sql = format!("SELECT post_id FROM votes WHERE user_id = ? AND post_id IN ({placeholders})");
                let mut q = sqlx::query_scalar::<_, String>(&sql).bind(uid);
                for p in &posts {
                    q = q.bind(&p.post.id);
                }
                let voted_ids: std::collections::HashSet<String> =
                    q.fetch_all(&self.pool).await.context("Failed to look up votes")?.into_iter().collect();
                for p in posts.iter_mut() {
                    p.user_voted = voted_ids.contains(&p.post.id);
                }
            }
        }

        Ok((posts, total))
    }

    /// List posts across ALL projects — admin moderation queue.
    ///
    /// Optional filters: exact status, exact project slug. Ordered newest first.
    /// Same join shape as `list_posts` so `PostDetail` maps identically.
    pub async fn list_all_posts(&self, params: ListAllPostsParams<'_>) -> Result<(Vec<PostDetail>, i64)> {
        let mut where_parts: Vec<&str> = vec!["1=1"];
        if params.status.is_some() {
            where_parts.push("p.status = ?");
        }
        if params.project_slug.is_some() {
            where_parts.push("pr.slug = ?");
        }

        let where_sql = where_parts.join(" AND ");

        // Count first (same WHERE, no join needed unless filtering by slug).
        let count_sql = format!(
            "SELECT COUNT(*) FROM posts p \
             LEFT JOIN projects pr ON p.project_id = pr.id \
             WHERE {where_sql}"
        );
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(s) = params.status {
            count_q = count_q.bind(status_to_str(s));
        }
        if let Some(slug) = params.project_slug {
            count_q = count_q.bind(slug);
        }
        let total = count_q.fetch_one(&self.pool).await.context("Failed to count posts")?;

        let sql = format!(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar, \
             pr.slug as project_slug, pr.name as project_name \
             FROM posts p \
             LEFT JOIN users u ON p.created_by = u.id \
             LEFT JOIN projects pr ON p.project_id = pr.id \
             WHERE {where_sql} \
             ORDER BY p.created_at DESC \
             LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query(&sql);
        if let Some(s) = params.status {
            q = q.bind(status_to_str(s));
        }
        if let Some(slug) = params.project_slug {
            q = q.bind(slug);
        }
        q = q.bind(params.limit).bind(params.offset);

        let rows = q.fetch_all(&self.pool).await.context("Failed to list posts")?;
        let posts: Vec<PostDetail> = rows.iter().map(map_post_detail).collect();

        Ok((posts, total))
    }

    /// Aggregate counts for a project — posts by status/category, unique
    /// participants, and vote/comment totals (used by the safe-delete dialog).
    pub async fn project_stats(&self, project_id: &str) -> Result<ProjectStats> {
        let by_status: std::collections::HashMap<String, i64> =
            sqlx::query("SELECT status, COUNT(*) as n FROM posts WHERE project_id = ? GROUP BY status")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?
                .iter()
                .map(|row| (row.get::<String, _>("status"), row.get::<i64, _>("n")))
                .collect();

        let by_category: std::collections::HashMap<String, i64> =
            sqlx::query("SELECT category, COUNT(*) as n FROM posts WHERE project_id = ? GROUP BY category")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?
                .iter()
                .map(|row| (row.get::<String, _>("category"), row.get::<i64, _>("n")))
                .collect();

        let total_posts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?;

        let total_users: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT created_by) FROM posts WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?;

        let total_votes: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM votes WHERE post_id IN (SELECT id FROM posts WHERE project_id = ?)",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;

        let total_comments: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM comments WHERE post_id IN (SELECT id FROM posts WHERE project_id = ?)",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(ProjectStats { total_posts, by_status, by_category, total_users, total_votes, total_comments })
    }

    /// Get a single post with detail.
    pub async fn get_post(&self, post_id: &str, user_id: Option<&str>) -> Result<Option<PostDetail>> {
        let row = sqlx::query(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM posts p \
             LEFT JOIN users u ON p.created_by = u.id \
             WHERE p.id = ?",
        )
        .bind(post_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get post")?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut detail = map_post_detail(&row);

        // Check if current user has voted
        if let Some(uid) = user_id {
            detail.user_voted = self.has_voted(uid, post_id).await.unwrap_or(false);
        }

        Ok(Some(detail))
    }

    /// Create a new post.
    pub async fn create_post(
        &self,
        project_id: &str,
        title: &str,
        description: &str,
        category: PostCategory,
        created_by: &str,
    ) -> Result<Post> {
        let id = super::new_id();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO posts (id, project_id, title, description, status, category, vote_count, comment_count, created_by, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'open', ?, 0, 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(title)
        .bind(description)
        .bind(category_to_str(category))
        .bind(created_by)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to create post")?;

        Ok(Post {
            id,
            project_id: project_id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: PostStatus::Open,
            category,
            vote_count: 0,
            comment_count: 0,
            created_by: created_by.to_string(),
            created_at: parse_now(&now),
            updated_at: parse_now(&now),
        })
    }

    /// Update post status.
    /// Update post status.
    pub async fn update_post_status(&self, post_id: &str, status: PostStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE posts SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status_to_str(status))
            .bind(&now)
            .bind(post_id)
            .execute(&self.pool)
            .await
            .context("Failed to update post status")?;
        Ok(())
    }

    /// Update post category.
    pub async fn update_post_category(&self, post_id: &str, category: PostCategory) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE posts SET category = ?, updated_at = ? WHERE id = ?")
            .bind(category_to_str(category))
            .bind(&now)
            .bind(post_id)
            .execute(&self.pool)
            .await
            .context("Failed to update post category")?;
        Ok(())
    }

    /// Delete a post.
    pub async fn delete_post(&self, post_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM posts WHERE id = ?")
            .bind(post_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete post")?;
        Ok(())
    }

    // ── Votes ────────────────────────────────────────────────────────

    /// Toggle vote on a post. Returns true if now voted, false if unvoted.
    /// Toggle vote on a post. Returns true if now voted, false if unvoted.
    /// Wrapped in a transaction to prevent race conditions.
    pub async fn toggle_vote(&self, user_id: &str, post_id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await.context("Failed to begin transaction")?;

        let existing: Option<(String,)> = sqlx::query_as("SELECT user_id FROM votes WHERE user_id = ? AND post_id = ?")
            .bind(user_id)
            .bind(post_id)
            .fetch_optional(&mut *tx)
            .await?;

        let voted = match existing {
            Some(_) => {
                sqlx::query("DELETE FROM votes WHERE user_id = ? AND post_id = ?")
                    .bind(user_id)
                    .bind(post_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE posts SET vote_count = MAX(0, vote_count - 1) WHERE id = ?")
                    .bind(post_id)
                    .execute(&mut *tx)
                    .await?;
                false
            }
            None => {
                let now = chrono::Utc::now().to_rfc3339();
                sqlx::query("INSERT INTO votes (user_id, post_id, created_at) VALUES (?, ?, ?)")
                    .bind(user_id)
                    .bind(post_id)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE posts SET vote_count = vote_count + 1 WHERE id = ?")
                    .bind(post_id)
                    .execute(&mut *tx)
                    .await?;
                true
            }
        };

        tx.commit().await.context("Failed to commit vote toggle")?;
        Ok(voted)
    }

    /// Check if a user has voted on a post.
    pub async fn has_voted(&self, user_id: &str, post_id: &str) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as("SELECT user_id FROM votes WHERE user_id = ? AND post_id = ?")
            .bind(user_id)
            .bind(post_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    // ── Comments ────────────────────────────────────────────────────

    /// List comments for a post, ordered oldest-first for threading.
    pub async fn list_comments(&self, post_id: &str) -> Result<Vec<CommentDetail>> {
        let rows = sqlx::query(
            "SELECT c.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM comments c \
             LEFT JOIN users u ON c.created_by = u.id \
             WHERE c.post_id = ? \
             ORDER BY c.created_at ASC",
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list comments")?;

        Ok(rows.iter().map(map_comment_detail).collect())
    }

    /// Get a single comment by ID (for ownership checks).
    pub async fn get_comment(&self, comment_id: &str) -> Result<Option<Comment>> {
        let row =
            sqlx::query("SELECT id, post_id, parent_id, content, created_by, created_at FROM comments WHERE id = ?")
                .bind(comment_id)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to get comment")?;

        Ok(row.as_ref().map(map_comment))
    }

    /// Add a comment.
    pub async fn create_comment(
        &self,
        post_id: &str,
        content: &str,
        parent_id: Option<&str>,
        created_by: &str,
    ) -> Result<CommentDetail> {
        let id = super::new_id();
        let now = Utc::now().to_rfc3339();

        let mut tx = self.pool.begin().await.context("Failed to begin transaction")?;

        sqlx::query(
            "INSERT INTO comments (id, post_id, parent_id, content, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(post_id)
        .bind(parent_id)
        .bind(content)
        .bind(created_by)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .context("Failed to create comment")?;

        sqlx::query("UPDATE posts SET comment_count = comment_count + 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(post_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await.context("Failed to commit comment creation")?;

        // Fetch creator info for the response
        let creator_row = sqlx::query("SELECT u.id, u.email, u.name, u.avatar_url FROM users u WHERE u.id = ?")
            .bind(created_by)
            .fetch_one(&self.pool)
            .await?;

        let creator = UserSummary {
            id: creator_row.get("id"),
            email: creator_row.get("email"),
            name: creator_row.get("name"),
            avatar_url: creator_row.get("avatar_url"),
        };

        Ok(CommentDetail {
            comment: Comment {
                id,
                post_id: post_id.to_string(),
                parent_id: parent_id.map(String::from),
                content: content.to_string(),
                created_by: created_by.to_string(),
                created_at: parse_now(&now),
            },
            creator,
        })
    }

    /// Delete a comment.
    /// Delete a comment and decrement the post's comment_count in a transaction.
    pub async fn delete_comment(&self, comment_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.context("Failed to begin transaction")?;

        // Get post_id before deleting
        let post_id: Option<String> = sqlx::query_scalar("SELECT post_id FROM comments WHERE id = ?")
            .bind(comment_id)
            .fetch_optional(&mut *tx)
            .await?;

        // Delete the comment
        let result = sqlx::query("DELETE FROM comments WHERE id = ?")
            .bind(comment_id)
            .execute(&mut *tx)
            .await
            .context("Failed to delete comment")?;

        // Decrement comment_count if a row was deleted
        if result.rows_affected() > 0 {
            if let Some(pid) = post_id {
                sqlx::query("UPDATE posts SET comment_count = MAX(0, comment_count - 1) WHERE id = ?")
                    .bind(pid)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await.context("Failed to commit comment deletion")?;
        Ok(())
    }

    // ── Users ───────────────────────────────────────────────────────

    /// Find or create user by email.
    pub async fn find_or_create_user(
        &self,
        email: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
        admin_emails: &[String],
    ) -> Result<User> {
        let email_lower = email.to_lowercase();
        let is_admin = admin_emails.iter().any(|e| e == &email_lower);
        let role = if is_admin { "admin" } else { "member" };

        // Check existing
        let row =
            sqlx::query("SELECT id, email, name, avatar_url, role, created_at, last_login FROM users WHERE email = ?")
                .bind(&email_lower)
                .fetch_optional(&self.pool)
                .await?;

        if let Some(ref row) = row {
            let mut user = map_user(row);
            let now = Utc::now().to_rfc3339();

            // Auto-promote to admin if in ADMIN_EMAILS (and not already admin)
            if is_admin && user.role != UserRole::Admin {
                sqlx::query("UPDATE users SET role = 'admin', last_login = ? WHERE id = ?")
                    .bind(&now)
                    .bind(&user.id)
                    .execute(&self.pool)
                    .await?;
                user.role = UserRole::Admin;
            } else {
                sqlx::query("UPDATE users SET last_login = ? WHERE id = ?")
                    .bind(&now)
                    .bind(&user.id)
                    .execute(&self.pool)
                    .await?;
            }

            user.last_login = parse_now(&now);
            Ok(user)
        } else {
            // Create new user
            let id = super::new_id();
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO users (id, email, name, avatar_url, role, created_at, last_login) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&email_lower)
            .bind(name.unwrap_or(""))
            .bind(avatar_url.unwrap_or(""))
            .bind(role)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to create user")?;

            Ok(User {
                id,
                email: email_lower.clone(),
                name: name.unwrap_or("").to_string(),
                avatar_url: avatar_url.unwrap_or("").to_string(),
                role: if is_admin { UserRole::Admin } else { UserRole::Member },
                created_at: parse_now(&now),
                last_login: parse_now(&now),
            })
        }
    }

    /// Get user by ID.
    pub async fn get_user(&self, user_id: &str) -> Result<Option<User>> {
        let row =
            sqlx::query("SELECT id, email, name, avatar_url, role, created_at, last_login FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.as_ref().map(map_user))
    }

    /// Get current user for session.
    pub async fn get_current_user(&self, user_id: &str) -> Result<CurrentUser> {
        let user = self.get_user(user_id).await?.context("User not found")?;
        Ok(CurrentUser { id: user.id, email: user.email, role: user.role })
    }

    /// Upsert user identity (provider link).
    pub async fn upsert_identity(&self, user_id: &str, provider: &str, provider_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO user_identities (user_id, provider, provider_id, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT(provider, provider_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(provider)
        .bind(provider_id)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Attachment methods ─────────────────────────────────────────────

    /// Create an attachment record in the database.
    pub async fn create_attachment(
        &self,
        post_id: &str,
        filename: &str,
        mime: &str,
        size: i64,
        storage_path: &str,
        created_by: &str,
    ) -> Result<rungu_proto::Attachment> {
        let id = super::new_id();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO post_attachments (id, post_id, filename, mime, size, storage_path, created_by, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(post_id)
        .bind(filename)
        .bind(mime)
        .bind(size)
        .bind(storage_path)
        .bind(created_by)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(rungu_proto::Attachment {
            id,
            post_id: post_id.to_string(),
            filename: filename.to_string(),
            mime: mime.to_string(),
            size,
            created_by: created_by.to_string(),
            created_at: now,
        })
    }

    /// List all attachments for a post.
    pub async fn list_attachments(&self, post_id: &str) -> Result<Vec<rungu_proto::Attachment>> {
        let rows = sqlx::query(
            "SELECT id, post_id, filename, mime, size, storage_path, created_by, created_at \
             FROM post_attachments WHERE post_id = ? ORDER BY created_at ASC",
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await?;

        let attachments = rows
            .iter()
            .map(|row| rungu_proto::Attachment {
                id: row.get("id"),
                post_id: row.get("post_id"),
                filename: row.get("filename"),
                mime: row.get("mime"),
                size: row.get("size"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(attachments)
    }

    /// Get a single attachment by ID (returns storage_path too).
    pub async fn get_attachment(&self, attachment_id: &str) -> Result<Option<(rungu_proto::Attachment, String)>> {
        let row = sqlx::query(
            "SELECT id, post_id, filename, mime, size, storage_path, created_by, created_at \
             FROM post_attachments WHERE id = ?",
        )
        .bind(attachment_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let storage_path: String = row.get("storage_path");
            let attachment = rungu_proto::Attachment {
                id: row.get("id"),
                post_id: row.get("post_id"),
                filename: row.get("filename"),
                mime: row.get("mime"),
                size: row.get("size"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
            };
            Ok(Some((attachment, storage_path)))
        } else {
            Ok(None)
        }
    }

    /// Delete an attachment record from the database. Returns the storage_path if found.
    pub async fn delete_attachment(&self, attachment_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT storage_path FROM post_attachments WHERE id = ?")
            .bind(attachment_id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let storage_path: String = row.get("storage_path");
            sqlx::query("DELETE FROM post_attachments WHERE id = ?").bind(attachment_id).execute(&self.pool).await?;
            Ok(Some(storage_path))
        } else {
            Ok(None)
        }
    }

    // ── Webhooks ──────────────────────────────────────────────────────

    /// Create a webhook subscription.
    pub async fn create_webhook(&self, project_id: &str, url: &str, events: &str, secret: &str) -> Result<Webhook> {
        let id = super::new_id();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO webhooks (id, project_id, url, secret, events, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(url)
        .bind(secret)
        .bind(events)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(Webhook {
            id,
            project_id: project_id.to_string(),
            url: url.to_string(),
            events: events.to_string(),
            is_active: true,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// List all webhooks for a project.
    pub async fn list_webhooks(&self, project_id: &str) -> Result<Vec<Webhook>> {
        let rows = sqlx::query("SELECT * FROM webhooks WHERE project_id = ? ORDER BY created_at DESC")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.iter().map(map_webhook).collect())
    }

    /// Get a single webhook by ID.
    pub async fn get_webhook(&self, webhook_id: &str) -> Result<Option<Webhook>> {
        let row =
            sqlx::query("SELECT * FROM webhooks WHERE id = ?").bind(webhook_id).fetch_optional(&self.pool).await?;

        Ok(row.as_ref().map(map_webhook))
    }

    /// Get the secret for a webhook (not exposed in API responses).
    pub async fn get_webhook_secret(&self, webhook_id: &str) -> Result<Option<String>> {
        let row =
            sqlx::query("SELECT secret FROM webhooks WHERE id = ?").bind(webhook_id).fetch_optional(&self.pool).await?;

        Ok(row.map(|r| r.get::<String, _>("secret")))
    }

    /// Update a webhook's URL, events, or active status.
    pub async fn update_webhook(
        &self,
        webhook_id: &str,
        url: Option<&str>,
        events: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Option<Webhook>> {
        // Fetch existing
        let existing = self.get_webhook(webhook_id).await?;
        let Some(mut wh) = existing else { return Ok(None) };

        if let Some(u) = url {
            wh.url = u.to_string();
        }
        if let Some(e) = events {
            wh.events = e.to_string();
        }
        if let Some(a) = is_active {
            wh.is_active = a;
        }

        let now = Utc::now().to_rfc3339();
        let active_int = if wh.is_active { 1 } else { 0 };
        sqlx::query("UPDATE webhooks SET url = ?, events = ?, is_active = ?, updated_at = ? WHERE id = ?")
            .bind(&wh.url)
            .bind(&wh.events)
            .bind(active_int)
            .bind(&now)
            .bind(webhook_id)
            .execute(&self.pool)
            .await?;

        wh.updated_at = now;
        Ok(Some(wh))
    }

    /// Delete a webhook.
    pub async fn delete_webhook(&self, webhook_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM webhooks WHERE id = ?").bind(webhook_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Get all active webhooks for a project that are subscribed to the given event.
    pub async fn get_active_webhooks_for_event(
        &self,
        project_id: &str,
        event_type: &str,
    ) -> Result<Vec<(Webhook, String)>> {
        let rows = sqlx::query(
            "SELECT w.*, w.secret FROM webhooks w WHERE w.project_id = ? AND w.is_active = 1 AND (w.events = '*' OR w.events LIKE ?)",
        )
        .bind(project_id)
        .bind(format!("%{event_type}%"))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let secret: String = r.get("secret");
                (map_webhook(r), secret)
            })
            .collect())
    }

    // ── Webhook Deliveries ────────────────────────────────────────────

    /// Record a webhook delivery attempt.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_webhook_delivery(
        &self,
        webhook_id: &str,
        event_type: &str,
        payload: &str,
        status_code: Option<i32>,
        success: bool,
        attempts: i32,
        last_error: &str,
    ) -> Result<String> {
        let id = super::new_id();
        let now = Utc::now().to_rfc3339();
        let success_int = if success { 1 } else { 0 };
        sqlx::query(
            "INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, status_code, success, attempts, last_error, created_at, delivered_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(webhook_id)
        .bind(event_type)
        .bind(payload)
        .bind(status_code)
        .bind(success_int)
        .bind(attempts)
        .bind(last_error)
        .bind(&now)
        .bind(if success { Some(now.clone()) } else { None })
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// List recent deliveries for a webhook.
    pub async fn list_webhook_deliveries(&self, webhook_id: &str, limit: i64) -> Result<Vec<WebhookDelivery>> {
        let rows =
            sqlx::query("SELECT * FROM webhook_deliveries WHERE webhook_id = ? ORDER BY created_at DESC LIMIT ?")
                .bind(webhook_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;

        Ok(rows.iter().map(map_webhook_delivery).collect())
    }
}

// ── Webhook row mappers ───────────────────────────────────────────────

fn map_webhook(row: &AnyRow) -> Webhook {
    Webhook {
        id: row.get("id"),
        project_id: row.get("project_id"),
        url: row.get("url"),
        events: row.get("events"),
        is_active: match row.try_get::<i64, _>("is_active") {
            Ok(v) => v != 0,
            Err(_) => row.get::<bool, _>("is_active"),
        },
        created_at: row.get::<String, _>("created_at"),
        updated_at: row.get::<String, _>("updated_at"),
    }
}

fn map_webhook_delivery(row: &AnyRow) -> WebhookDelivery {
    let success = match row.try_get::<i64, _>("success") {
        Ok(v) => v != 0,
        Err(_) => row.get::<bool, _>("success"),
    };

    WebhookDelivery {
        id: row.get("id"),
        webhook_id: row.get("webhook_id"),
        event_type: row.get("event_type"),
        payload: row.get("payload"),
        status_code: row.try_get("status_code").ok(),
        success,
        attempts: row.try_get("attempts").unwrap_or(0),
        last_error: row.try_get("last_error").unwrap_or_default(),
        created_at: row.get::<String, _>("created_at"),
        delivered_at: row.try_get::<String, _>("delivered_at").ok(),
    }
}
