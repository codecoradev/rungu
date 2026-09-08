//! # rungu-core
//!
//! Core logic — storage, business logic, database queries.
//! Supports SQLite and PostgreSQL via `sqlx::Any`.

use anyhow::{Context, Result};
use sqlx::AnyPool;

pub mod storage;
pub mod store;

pub use rungu_proto::ListAllPostsParams;
pub use storage::{ALLOWED_MIME_TYPES, FsStorage, MAX_UPLOAD_SIZE, Storage, create_storage, storage_key, verify_image};
pub use store::Store;

/// Open a database connection pool.
///
/// Detects the database type from the connection string:
/// - `sqlite:path.db` or `sqlite::memory:` → SQLite
/// - `postgres://user:pass@host/db` → PostgreSQL
pub async fn open_pool(database_url: &str) -> Result<AnyPool> {
    sqlx::any::install_default_drivers();
    let is_sqlite = database_url.starts_with("sqlite:");

    // In-memory SQLite: every connection is a SEPARATE empty database, so
    // the pool MUST stay at one connection or migrations/tables vanish.
    if is_sqlite && database_url.contains(":memory:") {
        let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new()
            .max_connections(1)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    let _ = sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await;
                    Ok(())
                })
            })
            .connect(database_url)
            .await?;
        return Ok(pool);
    }

    // SQLite file databases: ensure mode=rwc (read-write-create) in the URL.
    let database_url = if is_sqlite && !database_url.contains("mode=") {
        if database_url.contains('?') { format!("{database_url}&mode=rwc") } else { format!("{database_url}?mode=rwc") }
    } else {
        database_url.to_string()
    };

    // `PRAGMA foreign_keys` is per-connection in SQLite: a startup PRAGMA
    // covers only the one connection it ran on, leaving every other pooled
    // connection with FK constraints silently disabled (#190 scan). sqlx
    // does NOT accept `foreign_keys` as a URL parameter (AnyPool URL parse
    // error), so the correct mechanism is PoolOptions::after_connect —
    // the closure runs for EVERY connection the pool opens.
    let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new()
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                if is_sqlite {
                    // journal_mode/synchronous are also per-connection for
                    // Any; harmless to re-assert per connection.
                    let _ = sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await;
                    let _ = sqlx::query("PRAGMA synchronous=NORMAL").execute(&mut *conn).await;
                    let _ = sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await;
                }
                Ok(())
            })
        })
        .connect(&database_url)
        .await?;

    Ok(pool)
}

/// Run all database migrations.
/// Detects database type from the connection string and runs the appropriate SQL.
pub async fn run_migrations(pool: &AnyPool, database_url: &str) -> Result<()> {
    let is_sqlite = database_url.starts_with("sqlite:");
    let migrations = if is_sqlite {
        [
            include_str!("../migrations/sqlite/001_initial.sql"),
            include_str!("../migrations/sqlite/002_webhooks.sql"),
            include_str!("../migrations/sqlite/003_analytics.sql"),
            include_str!("../migrations/sqlite/004_official_response.sql"),
        ]
    } else {
        [
            include_str!("../migrations/postgres/001_initial.sql"),
            include_str!("../migrations/postgres/002_webhooks.sql"),
            include_str!("../migrations/postgres/003_analytics.sql"),
            include_str!("../migrations/postgres/004_official_response.sql"),
        ]
    };

    for sql in &migrations {
        sqlx::query(sql).execute(pool).await.context("Failed to run migration")?;
    }
    Ok(())
}

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Returns `true` when `database_url` refers to a SQLite backend.
///
/// Used by `Store` to decide whether it can use SQLite-only features (FTS5)
/// or must fall back to a portable query path. Detection is string-prefix
/// based because that's how the rest of the crate decides driver selection
/// too (see [`open_pool`]).
pub fn is_sqlite_url(database_url: &str) -> bool {
    database_url.starts_with("sqlite:")
}
