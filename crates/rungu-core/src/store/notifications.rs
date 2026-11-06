//! Email notification recipients (#73).

use super::*;

/// Minimal user projection for email delivery.
#[derive(Debug, Clone)]
pub struct NotifyUser {
    pub id: String,
    pub email: String,
    pub name: String,
}

impl Store {
    /// Who hears about activity on a post: its author plus everyone who
    /// commented, minus `actor_id`, anyone who opted out, and the synthetic
    /// `@rungu.local` bot users (MCP, API-key agent) — mail to them bounces.
    /// Distinct users.
    pub async fn notification_recipients(&self, post_id: &str, actor_id: &str) -> Result<Vec<NotifyUser>> {
        let rows = sqlx::query(
            "SELECT u.id, u.email, u.name FROM users u \
             WHERE (u.id = (SELECT created_by FROM posts WHERE id = ?) \
                 OR u.id IN (SELECT created_by FROM comments WHERE post_id = ?)) \
             AND u.id <> ? \
             AND u.email NOT LIKE '%@rungu.local' \
             AND u.id NOT IN (SELECT user_id FROM notification_opt_outs)",
        )
        .bind(post_id)
        .bind(post_id)
        .bind(actor_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch notification recipients")?;

        Ok(rows.iter().map(|r| NotifyUser { id: r.get("id"), email: r.get("email"), name: r.get("name") }).collect())
    }

    /// Whether a user opted out of email notifications.
    pub async fn notifications_opt_out(&self, user_id: &str) -> Result<bool> {
        let row = sqlx::query("SELECT user_id FROM notification_opt_outs WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to read notification opt-out")?;
        Ok(row.is_some())
    }

    /// Opt a user out of (or back into) email notifications. Idempotent.
    /// Returns false when the user does not exist.
    pub async fn set_notifications_opt_out(&self, user_id: &str, opt_out: bool) -> Result<bool> {
        if self.get_user(user_id).await?.is_none() {
            return Ok(false);
        }
        let sql = if opt_out {
            "INSERT INTO notification_opt_outs (user_id) VALUES (?) ON CONFLICT (user_id) DO NOTHING"
        } else {
            "DELETE FROM notification_opt_outs WHERE user_id = ?"
        };
        sqlx::query(sql).bind(user_id).execute(&self.pool).await.context("Failed to set notification opt-out")?;
        Ok(true)
    }
}
