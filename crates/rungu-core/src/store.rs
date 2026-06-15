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

    // ── Posts ───────────────────────────────────────────────────────

    /// List posts for a project with filters and sorting.
    pub async fn list_posts(&self, params: ListPostsParams<'_>) -> Result<(Vec<PostDetail>, i64)> {
        let mut where_clauses = vec!["project_id = ?1".to_string()];

        if let Some(s) = &params.status {
            where_clauses.push(format!("status = '{:?}'", s));
        }
        if let Some(c) = &params.category {
            where_clauses.push(format!("category = '{:?}'", c));
        }
        if let Some(q) = &params.query {
            where_clauses.push(format!("(title LIKE '%{q}%' OR description LIKE '%{q}%')"));
        }

        let where_sql = where_clauses.join(" AND ");

        // Total count
        let count_sql = format!("SELECT COUNT(*) as cnt FROM posts WHERE {where_sql}");
        let total: i64 =
            sqlx::query_scalar(&count_sql).bind(params.project_id).fetch_one(&self.pool).await.unwrap_or(0);

        // Sort
        let order = match params.sort {
            PostSort::Newest => "created_at DESC",
            PostSort::Oldest => "created_at ASC",
            PostSort::MostVotes => "vote_count DESC, created_at DESC",
            PostSort::LeastVotes => "vote_count ASC, created_at DESC",
            PostSort::RecentlyUpdated => "updated_at DESC",
        };

        let sql = format!(
            "SELECT p.*, u.id as user_id, u.email as user_email, u.name as user_name, u.avatar_url as user_avatar \
             FROM posts p \
             LEFT JOIN users u ON p.created_by = u.id \
             WHERE {where_sql} \
             ORDER BY {order} \
             LIMIT ?2 OFFSET ?3"
        );

        let rows = sqlx::query(&sql)
            .bind(params.project_id)
            .bind(params.limit)
            .bind(params.offset)
            .fetch_all(&self.pool)
            .await
            .context("Failed to list posts")?;

        // TODO(issue #3): map rows to PostDetail. Currently returns empty until #3 lands.
        let _ = rows;

        // Placeholder — full implementation in issue #3
        Ok((vec![], total))
    }

    /// Get a single post with detail.
    pub async fn get_post(&self, _post_id: &str, _user_id: Option<&str>) -> Result<Option<PostDetail>> {
        // TODO: implement with full join
        Ok(None)
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

    /// List comments for a post.
    pub async fn list_comments(&self, _post_id: &str) -> Result<Vec<CommentDetail>> {
        // TODO: implement with user join
        Ok(vec![])
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
