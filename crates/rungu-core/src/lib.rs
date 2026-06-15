//! # rungu-core
//!
//! Core logic — storage, business logic, SQLite queries.

use anyhow::Result;
use chrono::Utc;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

pub mod store;

pub use store::Store;

/// Open a SQLite connection pool with WAL mode.
pub async fn open_pool(db_path: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
    Ok(pool)
}

/// Run all database migrations.
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(include_str!("../../crates/rungud/migrations/001_initial.sql")).execute(pool).await?;
    Ok(())
}

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}
