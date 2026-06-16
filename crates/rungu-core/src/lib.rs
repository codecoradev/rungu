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

    if database_url.starts_with("sqlite:") {
        // SQLite — connect directly. WAL mode only works for file-based DBs.
        // In-memory databases (sqlite::memory:) skip WAL params.
        let pool = if database_url.contains(":memory:") {
            // In-memory: use AnyPoolOptions to limit to 1 connection
            // (each connection to in-memory SQLite gets its own DB)
            sqlx::pool::PoolOptions::<sqlx::Any>::new().max_connections(1).connect(database_url).await?
        } else {
            // File-based SQLite — enable WAL mode via query params
            let wal_url = if database_url.contains("?") {
                format!("{}&_journal_mode=WAL&_synchronous=NORMAL", database_url)
            } else {
                format!("{}?mode=rwc&_journal_mode=WAL&_synchronous=NORMAL", database_url)
            };
            AnyPool::connect(&wal_url).await?
        };
        Ok(pool)
    } else {
        // PostgreSQL
        let pool = AnyPool::connect(database_url).await?;
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
