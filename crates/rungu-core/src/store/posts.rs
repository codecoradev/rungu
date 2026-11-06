//! Posts, search, similarity and votes.

use super::*;

impl Store {
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
            // Trending (#203): recent vote velocity from analytics events via a
            // correlated scalar subquery (no aggregate-in-ORDER-BY, so it works
            // without GROUP BY on both dialects). Posts without events score 0
            // and fall back to total votes, then recency.
            PostSort::Trending => {
                if self.is_sqlite {
                    "(SELECT COUNT(*) FROM analytics_events ae WHERE ae.post_id = p.id AND ae.event_type = 'vote' AND ae.created_at >= datetime('now', '-7 days')) DESC, p.vote_count DESC, p.created_at DESC"
                } else {
                    "(SELECT COUNT(*) FROM analytics_events ae WHERE ae.post_id = p.id AND ae.event_type = 'vote' AND ae.created_at >= NOW() - INTERVAL '7 days') DESC, p.vote_count DESC, p.created_at DESC"
                }
            }
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
            "SELECT p.*, u.id as user_id, u.name as user_name, u.avatar_url as user_avatar \
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
            "SELECT p.*, u.id as user_id, u.name as user_name, u.avatar_url as user_avatar, \
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
            "SELECT p.*, u.id as user_id, u.name as user_name, u.avatar_url as user_avatar \
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
        let id = crate::new_id();
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
}
