//! # store
//!
//! SQLite storage operations for Rungu.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rungu_proto::*;
use sqlx::{Row, SqlitePool};

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
fn map_project(row: &sqlx::sqlite::SqliteRow) -> Project {
    Project {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        description: row.get("description"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
    }
}

/// Map a SQLite row to a User.
fn map_user(row: &sqlx::sqlite::SqliteRow) -> User {
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
fn map_post_detail(row: &sqlx::sqlite::SqliteRow) -> PostDetail {
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
fn map_post(row: &sqlx::sqlite::SqliteRow) -> Post {
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
fn map_comment(row: &sqlx::sqlite::SqliteRow) -> Comment {
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
fn map_comment_detail(row: &sqlx::sqlite::SqliteRow) -> CommentDetail {
    let comment = map_comment(row);
    let creator = UserSummary {
        id: row.get("user_id"),
        email: row.get("user_email"),
        name: row.get("user_name"),
        avatar_url: row.get("user_avatar"),
    };
    CommentDetail { comment, creator }
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
    pool: SqlitePool,
}

impl Store {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the pool.
    pub fn pool(&self) -> &SqlitePool {
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
    pub async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        let row = sqlx::query("SELECT id, slug, name, description, created_at FROM projects WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get project")?;
        Ok(row.as_ref().map(map_project))
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
        Ok(())
    }

    // ── Posts ───────────────────────────────────────────────────────────

    /// List posts for a project with filters and sorting.
    ///
    /// Uses positional `?` parameter binding — user input is NEVER interpolated into SQL.
    /// The search query uses `LIKE ?` with the pattern passed as a bind parameter.
    pub async fn list_posts(&self, params: ListPostsParams<'_>) -> Result<(Vec<PostDetail>, i64)> {
        // Build WHERE clause fragments — only add conditions for filters that are present.
        // Each `?` is a positional placeholder bound later via .bind().
        let mut conditions = vec!["project_id = ?".to_string()];

        if params.status.is_some() {
            conditions.push("status = ?".to_string());
        }
        if params.category.is_some() {
            conditions.push("category = ?".to_string());
        }
        if params.query.is_some() {
            // Same pattern bound twice for title OR description LIKE
            conditions.push("(title LIKE ? OR description LIKE ?)".to_string());
        }

        let where_sql = conditions.join(" AND ");

        // Sort — safe because it's a hardcoded match, not user input
        let order = match params.sort {
            PostSort::Newest => "created_at DESC",
            PostSort::Oldest => "created_at ASC",
            PostSort::MostVotes => "vote_count DESC, created_at DESC",
            PostSort::LeastVotes => "vote_count ASC, created_at DESC",
            PostSort::RecentlyUpdated => "updated_at DESC",
        };

        // Helper: bind optional filter values in order (project_id already first)
        // Returns the next bind index (for limit/offset appended after).
        macro_rules! bind_filters {
            ($query:expr) => {{
                let q = $query.bind(params.project_id);
                let q = if let Some(ref s) = params.status { q.bind(format!("{:?}", s).to_lowercase()) } else { q };
                let q = if let Some(ref c) = params.category { q.bind(format!("{:?}", c).to_lowercase()) } else { q };
                let q = if let Some(query_text) = params.query {
                    let pattern = format!("%{query_text}%");
                    // Bind twice: once for title LIKE, once for description LIKE
                    q.bind(pattern.clone()).bind(pattern)
                } else {
                    q
                };
                q
            }};
        }

        // Count query (same WHERE, no LIMIT/OFFSET)
        let count_sql = format!("SELECT COUNT(*) FROM posts WHERE {where_sql}");
        let total: i64 = bind_filters!(sqlx::query_scalar::<_, i64>(&count_sql))
            .fetch_one(&self.pool)
            .await
            .context("Failed to count posts")?;

        // Main query with LIMIT/OFFSET appended
        let sql = format!(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM posts p \
             LEFT JOIN users u ON p.created_by = u.id \
             WHERE {where_sql} \
             ORDER BY {order} \
             LIMIT ? OFFSET ?"
        );

        let query = bind_filters!(sqlx::query(&sql)).bind(params.limit).bind(params.offset);

        let rows = query.fetch_all(&self.pool).await.context("Failed to list posts")?;

        let posts = rows.iter().map(map_post_detail).collect();
        Ok((posts, total))
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
        .bind(format!("{:?}", category).to_lowercase())
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
    pub async fn update_post_status(&self, post_id: &str, status: PostStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE posts SET status = ?, updated_at = ? WHERE id = ?")
            .bind(format!("{:?}", status).to_lowercase())
            .bind(&now)
            .bind(post_id)
            .execute(&self.pool)
            .await
            .context("Failed to update post status")?;
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
                sqlx::query("INSERT INTO votes (user_id, post_id, created_at) VALUES (?, ?, datetime('now'))")
                    .bind(user_id)
                    .bind(post_id)
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
    ) -> Result<Comment> {
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

        Ok(Comment {
            id,
            post_id: post_id.to_string(),
            parent_id: parent_id.map(String::from),
            content: content.to_string(),
            created_by: created_by.to_string(),
            created_at: parse_now(&now),
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
        let is_admin = admin_emails.iter().any(|e| e == &email.to_lowercase());
        let role = if is_admin { "admin" } else { "member" };

        // Check existing
        let row =
            sqlx::query("SELECT id, email, name, avatar_url, role, created_at, last_login FROM users WHERE email = ?")
                .bind(email)
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
            .bind(email)
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
                email: email.to_string(),
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
        sqlx::query(
            "INSERT INTO user_identities (user_id, provider, provider_id, created_at) VALUES (?, ?, ?, datetime('now')) \
             ON CONFLICT(provider, provider_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(provider)
        .bind(provider_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
