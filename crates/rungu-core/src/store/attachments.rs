//! Attachment rows.

use super::*;

impl Store {
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
        let id = crate::new_id();
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

    /// Storage paths of every attachment on every post in a project, so the
    /// files can be removed when the project is deleted (the FK cascade only
    /// removes the rows).
    pub async fn list_project_attachment_paths(&self, project_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT a.storage_path FROM post_attachments a JOIN posts p ON a.post_id = p.id WHERE p.project_id = ?",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list project attachment paths")?;
        Ok(rows.iter().map(|r| r.get("storage_path")).collect())
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
}
