//! # store
//!
//! SQLite storage operations for Rungu.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rungu_proto::*;
use sqlx::{Row, SqlitePool};

/// Parse RFC3339 timestamp from SQLite TEXT column with fallback to datetime('now') format.
fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
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
            created_at: now.parse().unwrap(),
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
    /// Uses parameterized queries throughout — no string interpolation of user input.
    pub async fn list_posts(&self, params: ListPostsParams<'_>) -> Result<(Vec<PostDetail>, i64)> {
        // Build WHERE clause with parameterized placeholders to prevent SQL injection.
        // User-supplied search query (q) is passed as a bind parameter, never interpolated.
        let mut conditions = vec!["project_id = ?".to_string()];
        let mut bind_idx = 1u32; // SQLite parameter index (1-based for first param)

        if params.status.is_some() {
            bind_idx += 1;
            conditions.push(format!("status = ?{}", bind_idx));
        }
        if params.category.is_some() {
            bind_idx += 1;
            conditions.push(format!("category = ?{}", bind_idx));
        }
        if params.query.is_some() {
            // Two LIKE params for title + description
            bind_idx += 1;
            conditions.push(format!("(title LIKE ?{} OR description LIKE ?{})", bind_idx, bind_idx));
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

        // Build count query with same WHERE clause (no LIMIT/OFFSET)
        let count_sql = format!("SELECT COUNT(*) as cnt FROM posts WHERE {where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(params.project_id);
        let mut next_idx = 2u32;

        if let Some(ref s) = params.status {
            count_query = count_query.bind(format!("{:?}", s).to_lowercase());
            next_idx += 1;
        }
        if let Some(ref c) = params.category {
            count_query = count_query.bind(format!("{:?}", c).to_lowercase());
            next_idx += 1;
        }
        if let Some(q) = params.query {
            let pattern = format!("%{q}%");
            count_query = count_query.bind(pattern.clone()).bind(pattern);
            next_idx += 1;
        }

        let total = count_query.fetch_one(&self.pool).await.unwrap_or(0);

        // Build main query
        let limit_idx = next_idx;
        let offset_idx = next_idx + 1;
        let sql = format!(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM posts p \
             LEFT JOIN users u ON p.created_by = u.id \
             WHERE {where_sql} \
             ORDER BY {order} \
             LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        );

        let mut query = sqlx::query(&sql).bind(params.project_id);

        if let Some(ref s) = params.status {
            query = query.bind(format!("{:?}", s).to_lowercase());
        }
        if let Some(ref c) = params.category {
            query = query.bind(format!("{:?}", c).to_lowercase());
        }
        if let Some(q) = params.query {
            let pattern = format!("%{q}%");
            query = query.bind(pattern.clone()).bind(pattern);
        }

        query = query.bind(params.limit).bind(params.offset);

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
            created_at: now.parse().unwrap(),
            updated_at: now.parse().unwrap(),
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
    pub async fn toggle_vote(&self, user_id: &str, post_id: &str) -> Result<bool> {
        // Check if already voted
        let existing: Option<(String,)> = sqlx::query_as("SELECT user_id FROM votes WHERE user_id = ? AND post_id = ?")
            .bind(user_id)
            .bind(post_id)
            .fetch_optional(&self.pool)
            .await?;

        match existing {
            Some(_) => {
                // Unvote
                sqlx::query("DELETE FROM votes WHERE user_id = ? AND post_id = ?")
                    .bind(user_id)
                    .bind(post_id)
                    .execute(&self.pool)
                    .await?;
                sqlx::query("UPDATE posts SET vote_count = MAX(0, vote_count - 1) WHERE id = ?")
                    .bind(post_id)
                    .execute(&self.pool)
                    .await?;
                Ok(false)
            }
            None => {
                // Vote
                sqlx::query("INSERT INTO votes (user_id, post_id, created_at) VALUES (?, ?, datetime('now'))")
                    .bind(user_id)
                    .bind(post_id)
                    .execute(&self.pool)
                    .await?;
                sqlx::query("UPDATE posts SET vote_count = vote_count + 1 WHERE id = ?")
                    .bind(post_id)
                    .execute(&self.pool)
                    .await?;
                Ok(true)
            }
        }
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
        sqlx::query(
            "INSERT INTO comments (id, post_id, parent_id, content, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(post_id)
        .bind(parent_id)
        .bind(content)
        .bind(created_by)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to create comment")?;

        // Increment comment count
        sqlx::query("UPDATE posts SET comment_count = comment_count + 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        Ok(Comment {
            id,
            post_id: post_id.to_string(),
            parent_id: parent_id.map(String::from),
            content: content.to_string(),
            created_by: created_by.to_string(),
            created_at: now.parse().unwrap(),
        })
    }

    /// Delete a comment.
    pub async fn delete_comment(&self, comment_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM comments WHERE id = ?")
            .bind(comment_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete comment")?;
        Ok(())
    }

    // ── Users ───────────────────────────────────────────────────────

    /// Find or create user by email.
    pub async fn find_or_create_user(&self, email: &str, name: Option<&str>, avatar_url: Option<&str>) -> Result<User> {
        // Check existing
        let row =
            sqlx::query("SELECT id, email, name, avatar_url, role, created_at, last_login FROM users WHERE email = ?")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;

        if let Some(ref row) = row {
            let user = map_user(row);
            // Update last_login
            let now = Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET last_login = ? WHERE id = ?")
                .bind(&now)
                .bind(&user.id)
                .execute(&self.pool)
                .await?;
            Ok(user)
        } else {
            // Create new user
            let id = super::new_id();
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO users (id, email, name, avatar_url, role, created_at, last_login) VALUES (?, ?, ?, ?, 'member', ?, ?)",
            )
            .bind(&id)
            .bind(email)
            .bind(name.unwrap_or(""))
            .bind(avatar_url.unwrap_or(""))
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
                role: UserRole::Member,
                created_at: now.parse().unwrap(),
                last_login: now.parse().unwrap(),
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
