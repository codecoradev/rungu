//! # rungu-core
//!
//! Core logic — storage, business logic, database queries.
//! Supports SQLite and PostgreSQL via `sqlx::Any`.

use anyhow::{Context, Result};
use sqlx::AnyPool;

pub mod store;

pub use store::Store;

/// Open a database connection pool.
///
/// Detects the database type from the connection string:
/// - `sqlite:path.db` or `sqlite::memory:` → SQLite
/// - `postgres://user:pass@host/db` → PostgreSQL
pub async fn open_pool(database_url: &str) -> Result<AnyPool> {
    sqlx::any::install_default_drivers();

    if database_url.starts_with("sqlite:") && database_url.contains(":memory:") {
        // In-memory: single connection (each connection gets its own DB)
        let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new().max_connections(1).connect(database_url).await?;
        Ok(pool)
    } else {
        // SQLite file or PostgreSQL — connect via AnyPool
        let pool = AnyPool::connect(database_url).await?;
        // Enable WAL mode for SQLite file databases
        if database_url.starts_with("sqlite:") {
            let _ = sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await;
            let _ = sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await;
        }
        Ok(pool)
    }
}

/// Run all database migrations.
/// Detects database type from the connection string and runs the appropriate SQL.
pub async fn run_migrations(pool: &AnyPool, database_url: &str) -> Result<()> {
    let sql = if database_url.starts_with("sqlite:") {
        include_str!("../migrations/sqlite/001_initial.sql")
    } else {
        include_str!("../migrations/postgres/001_initial.sql")
    };

    sqlx::query(sql).execute(pool).await.context("Failed to run migrations")?;
    Ok(())
}

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
