//! Webhooks and their delivery log.

use super::*;

impl Store {
    // ── Webhooks ──────────────────────────────────────────────────────

    /// Create a webhook subscription.
    pub async fn create_webhook(&self, project_id: &str, url: &str, events: &str, secret: &str) -> Result<Webhook> {
        let id = crate::new_id();
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
        let id = crate::new_id();
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
