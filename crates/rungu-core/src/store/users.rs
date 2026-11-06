//! Users and identities.

use super::*;

impl Store {
    // ── Users ───────────────────────────────────────────────────────

    /// Find or create user by email.
    pub async fn find_or_create_user(
        &self,
        email: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
        admin_emails: &[String],
    ) -> Result<User> {
        let email_lower = email.to_lowercase();
        let is_admin = admin_emails.iter().any(|e| e == &email_lower);
        let role = if is_admin { "admin" } else { "member" };

        // Check existing
        let row =
            sqlx::query("SELECT id, email, name, avatar_url, role, created_at, last_login FROM users WHERE email = ?")
                .bind(&email_lower)
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
            let id = crate::new_id();
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO users (id, email, name, avatar_url, role, created_at, last_login) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&email_lower)
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
                email: email_lower.clone(),
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
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO user_identities (user_id, provider, provider_id, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT(provider, provider_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(provider)
        .bind(provider_id)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
