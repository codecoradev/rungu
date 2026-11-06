//! Analytics events (#186).

use super::*;

impl Store {
    // ── Analytics events (#186) ─────────────────────────────────────────

    /// Record an analytics event. Privacy-first: no IP, cookie, or
    /// fingerprint is ever stored — only aggregate counters.
    pub async fn record_event(&self, project_id: &str, post_id: Option<&str>, event_type: &str) -> Result<()> {
        sqlx::query("INSERT INTO analytics_events (id, project_id, post_id, event_type) VALUES (?, ?, ?, ?)")
            .bind(crate::new_id())
            .bind(project_id)
            .bind(post_id)
            .bind(event_type)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Aggregate event counts per type for a project within the last `days`
    /// days (0 = all time).
    pub async fn analytics_totals(
        &self,
        project_id: &str,
        days: u32,
    ) -> Result<std::collections::HashMap<String, i64>> {
        // Dialect note: `datetime('now', …)` is SQLite-only; PostgreSQL needs
        // a cast interval (same branch pattern as FTS5 vs tsvector search).
        let rows = if days == 0 {
            sqlx::query(
                "SELECT event_type, COUNT(*) as n FROM analytics_events WHERE project_id = ? GROUP BY event_type",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?
        } else if self.is_sqlite {
            sqlx::query(
                "SELECT event_type, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND created_at >= datetime('now', ?) GROUP BY event_type",
            )
            .bind(project_id)
            .bind(format!("-{} days", days))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT event_type, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND created_at >= NOW() - (? || ' days')::interval GROUP BY event_type",
            )
            .bind(project_id)
            .bind(days.to_string())
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.iter().map(|row| (row.get::<String, _>("event_type"), row.get::<i64, _>("n"))).collect())
    }

    /// Daily event counts for the last `days` days, bucketed by UTC date.
    /// Returns rows of (day, event_type, count).
    pub async fn analytics_daily(&self, project_id: &str, days: u32) -> Result<Vec<(String, String, i64)>> {
        let rows = if self.is_sqlite {
            sqlx::query(
                "SELECT date(created_at) as day, event_type, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND created_at >= datetime('now', ?) GROUP BY day, event_type ORDER BY day",
            )
            .bind(project_id)
            .bind(format!("-{} days", days))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT to_char(created_at, 'YYYY-MM-DD') as day, event_type, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND created_at >= NOW() - (? || ' days')::interval GROUP BY day, event_type ORDER BY day",
            )
            .bind(project_id)
            .bind(days.to_string())
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows
            .iter()
            .map(|row| (row.get::<String, _>("day"), row.get::<String, _>("event_type"), row.get::<i64, _>("n")))
            .collect())
    }

    /// Top posts by view count within the last `days` days (0 = all time).
    /// Returns (post_id, view_count) pairs, most-viewed first.
    pub async fn analytics_top_posts(&self, project_id: &str, days: u32, limit: i64) -> Result<Vec<(String, i64)>> {
        let rows = if days == 0 {
            sqlx::query(
                "SELECT post_id, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND event_type = 'post_view' AND post_id IS NOT NULL GROUP BY post_id ORDER BY n DESC LIMIT ?",
            )
            .bind(project_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else if self.is_sqlite {
            sqlx::query(
                "SELECT post_id, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND event_type = 'post_view' AND post_id IS NOT NULL AND created_at >= datetime('now', ?) GROUP BY post_id ORDER BY n DESC LIMIT ?",
            )
            .bind(project_id)
            .bind(format!("-{} days", days))
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT post_id, COUNT(*) as n FROM analytics_events WHERE project_id = ? AND event_type = 'post_view' AND post_id IS NOT NULL AND created_at >= NOW() - (? || ' days')::interval GROUP BY post_id ORDER BY n DESC LIMIT ?",
            )
            .bind(project_id)
            .bind(days.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.iter().map(|row| (row.get::<String, _>("post_id"), row.get::<i64, _>("n"))).collect())
    }
}
