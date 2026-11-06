//! Project queries (with the slug cache).

use super::*;

impl Store {
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

    /// Whether a project with this slug exists, read straight from the
    /// database. Use this for uniqueness checks: the slug cache can still hold
    /// a project that another process (e.g. `rungu mcp`) has deleted.
    pub async fn project_slug_exists(&self, slug: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 AS hit FROM projects WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to check project slug")?;
        Ok(row.is_some())
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
        let id = crate::new_id();
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
}
