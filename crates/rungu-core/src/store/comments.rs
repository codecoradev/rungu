//! Comment queries.

use super::*;

impl Store {
    // ── Comments ────────────────────────────────────────────────────

    /// List comments for a post, ordered oldest-first for threading.
    pub async fn list_comments(&self, post_id: &str) -> Result<Vec<CommentDetail>> {
        let rows = sqlx::query(
            "SELECT c.*, u.id as user_id, u.name as user_name, u.avatar_url as user_avatar \
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
        let id = crate::new_id();
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
        let creator_row = sqlx::query("SELECT u.id, u.name, u.avatar_url FROM users u WHERE u.id = ?")
            .bind(created_by)
            .fetch_one(&self.pool)
            .await?;

        let creator = UserSummary {
            id: creator_row.get("id"),
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
    /// Find posts in a project whose title or description share keywords with
    /// `title`, most-voted first. Backs both the post-detail "Similar posts"
    /// block and the as-you-type duplicate check in the New Post form.
    /// `exclude_id` drops one post (the post being viewed) from the results.
    pub async fn find_similar_by_title(
        &self,
        project_id: &str,
        title: &str,
        exclude_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PostDetail>> {
        // Deliberately LIKE-based, not FTS5: sqlx's AnyDriver fails to prepare
        // the FTS5 count query when the MATCH expression contains OR groups
        // (reproduced live, #206). OR-of-keywords is the right semantic for
        // dedup ("similar"), and LIKE covers both dialects identically.
        // Candidates match any keyword; they are ranked by how many keywords
        // they share (a title hit counts double), then by votes.
        let mut results = Vec::new();
        let tokens = similarity_tokens(title);
        if !tokens.is_empty() {
            let op = if self.is_sqlite { "LIKE" } else { "ILIKE" };
            let match_clause = tokens
                .iter()
                .map(|_| format!("(p.title {op} ? OR p.description {op} ?)"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let score = tokens
                .iter()
                .map(|_| {
                    format!(
                        "(CASE WHEN p.title {op} ? THEN 2 ELSE 0 END + CASE WHEN p.description {op} ? THEN 1 ELSE 0 END)"
                    )
                })
                .collect::<Vec<_>>()
                .join(" + ");
            let sql = format!(
                "SELECT p.*, u.id as user_id, u.name as user_name, u.avatar_url as user_avatar \
                 FROM posts p \
                 LEFT JOIN users u ON p.created_by = u.id \
                 WHERE p.project_id = ? AND p.id != ? AND ({match_clause}) \
                 ORDER BY ({score}) DESC, p.vote_count DESC, p.created_at DESC LIMIT ?"
            );
            // Post ids are UUIDs, never empty, so "" excludes nothing.
            let mut q = sqlx::query(&sql).bind(project_id).bind(exclude_id.unwrap_or(""));
            // Placeholders in SQL order: WHERE pairs, then ORDER BY pairs.
            for _pass in 0..2 {
                for t in &tokens {
                    let like = format!("%{t}%");
                    q = q.bind(like.clone()).bind(like);
                }
            }
            let rows = q.bind(limit).fetch_all(&self.pool).await.context("Failed to find similar posts")?;
            for row in rows {
                let id: String = row.try_get("id")?;
                let title: String = row.try_get("title")?;
                let description: Option<String> = row.try_get("description")?;
                let category = parse_category(&row.try_get::<String, _>("category")?);
                let status = parse_status(&row.try_get::<String, _>("status")?);
                let vote_count: i64 = row.try_get("vote_count")?;
                let comment_count: i64 = row.try_get("comment_count")?;
                let created_at: String = row.try_get("created_at")?;
                let updated_at: String = row.try_get("updated_at")?;
                let created_by: String = row.try_get("created_by")?;
                let uid: Option<String> = row.try_get("user_id")?;
                let uname: Option<String> = row.try_get("user_name")?;
                let uavatar: Option<String> = row.try_get("user_avatar")?;
                results.push(PostDetail {
                    post: rungu_proto::Post {
                        id,
                        project_id: project_id.to_string(),
                        title,
                        description: description.unwrap_or_default(),
                        category,
                        status,
                        vote_count,
                        comment_count,
                        created_by,
                        created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                            .map(|t| t.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
                            .map(|t| t.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    },
                    creator: rungu_proto::UserSummary {
                        id: uid.unwrap_or_default(),
                        name: uname.unwrap_or_else(|| "User".to_string()),
                        avatar_url: uavatar.unwrap_or_default(),
                    },
                    user_voted: false,
                });
            }
        }
        Ok(results)
    }

    /// Set (or clear, with `None`) the official team response comment on a post (#205).
    /// Returns the previous value so callers can report changes accurately.
    pub async fn set_official_response(&self, post_id: &str, comment_id: Option<&str>) -> Result<Option<String>> {
        let previous: Option<String> =
            sqlx::query_scalar("SELECT comment_id FROM post_official_response WHERE post_id = ?")
                .bind(post_id)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to read official response")?
                .flatten();

        if let Some(cid) = comment_id {
            // The comment must belong to the post — otherwise the pinned
            // block could render a comment from an entirely different thread.
            let belongs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM comments WHERE id = ? AND post_id = ?")
                .bind(cid)
                .bind(post_id)
                .fetch_one(&self.pool)
                .await
                .context("Failed to check comment")?;
            if belongs == 0 {
                anyhow::bail!("Comment does not belong to this post");
            }
        }

        sqlx::query(
            "INSERT INTO post_official_response (post_id, comment_id, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (post_id) DO UPDATE SET comment_id = excluded.comment_id, updated_at = excluded.updated_at",
        )
        .bind(post_id)
        .bind(comment_id)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to set official response")?;
        Ok(previous)
    }

    /// Fetch the official response comment (with author) for a post, if set (#205).
    pub async fn get_official_response(&self, post_id: &str) -> Result<Option<CommentDetail>> {
        let id: Option<String> = sqlx::query_scalar("SELECT comment_id FROM post_official_response WHERE post_id = ?")
            .bind(post_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to read official response")?
            .flatten();
        match id {
            Some(cid) => {
                let comment = sqlx::query_as::<
                    _,
                    (String, Option<String>, String, String, String, Option<String>, Option<String>, Option<String>),
                >(
                    "SELECT c.id, c.parent_id, c.content, c.created_by, c.created_at, \
                     u.id as user_id, u.name as user_name, u.avatar_url as user_avatar \
                     FROM comments c LEFT JOIN users u ON c.created_by = u.id WHERE c.id = ?",
                )
                .bind(&cid)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to fetch official response")?;
                Ok(comment.map(|(id, parent_id, content, created_by, created_at, uid, uname, uavatar)| CommentDetail {
                    comment: rungu_proto::Comment {
                        id,
                        post_id: post_id.to_string(),
                        parent_id,
                        content,
                        created_by,
                        created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                            .map(|t| t.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    },
                    creator: rungu_proto::UserSummary {
                        id: uid.unwrap_or_default(),
                        name: uname.unwrap_or_else(|| "User".to_string()),
                        avatar_url: uavatar.unwrap_or_default(),
                    },
                }))
            }
            None => Ok(None),
        }
    }

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
}
