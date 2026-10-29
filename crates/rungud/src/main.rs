//! # rungud — Rungu daemon
//!
//! Main binary: CLI subcommands + HTTP server.

pub mod config;
pub mod embed;
pub mod ratelimit;
pub mod server;
pub mod spa;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::prelude::*;

#[derive(Parser, Debug)]
#[command(name = "rungu", version, about = "Rungu — lightweight feedback board daemon")]
struct Cli {
    /// Database path (SQLite)
    #[arg(short, long, global = true, default_value = "rungu.db")]
    db: PathBuf,

    /// Log level
    #[arg(short = 'g', long, global = true, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the HTTP server (default)
    Serve {
        /// HTTP listen address
        #[arg(short, long, default_value = "0.0.0.0:3000")]
        listen: String,
    },
    /// List all projects
    ProjectList,
    /// Add a new project
    ProjectAdd {
        /// Project name
        name: String,
        /// Project slug (optional, auto-generated from name)
        #[arg(short, long)]
        slug: Option<String>,
        /// Project description
        #[arg(short = 'D', long, default_value = "")]
        description: String,
    },
    /// Health check (exit 0 if healthy)
    Healthcheck,
    /// Start MCP server (stdio JSON-RPC 2.0)
    Mcp,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init Sentry (must happen BEFORE tokio runtime).
    // If SENTRY_DSN is not set, Sentry is a no-op — no events are sent.
    let _sentry_guard = {
        let mut opts = sentry::ClientOptions::default();
        if let Ok(dsn_str) = std::env::var("SENTRY_DSN") {
            opts.dsn = dsn_str.parse().ok();
        }
        opts.release = sentry::release_name!();
        sentry::init(opts)
    };

    let sentry_active = sentry::Hub::main().client().is_some();
    if sentry_active {
        info!("Sentry enabled (DSN configured)");
    } else {
        info!("Sentry disabled (no SENTRY_DSN)");
    }

    // Init tracing (with Sentry bridge if active).
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| cli.log_level.clone().into());
    let registry = tracing_subscriber::registry().with(filter);
    // Logs go to stderr: in `rungu mcp` stdout is the JSON-RPC stream, and
    // any log line written there corrupts the protocol for the client.
    if sentry_active {
        registry
            .with(sentry_tracing::layer())
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();
    } else {
        registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr)).init();
    }

    // Manual tokio runtime — required because sentry::init() must
    // precede the runtime, and #[tokio::main] hides the construction.
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    rt.block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> Result<()> {
    // If no explicit --db or DATABASE_URL, use standardised data directory.
    let db_path = if cli.db.to_string_lossy() == "rungu.db" {
        let dir = config::Config::default_data_dir();
        config::Config::ensure_data_dir(&dir).expect("failed to create data directory");
        config::Config::default_db_path().into()
    } else {
        cli.db.clone()
    };
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| format!("sqlite:{}", db_path.display()));
    let pool = rungu_core::open_pool(&db_url).await?;
    rungu_core::run_migrations(&pool, &db_url).await?;
    info!("Database ready: {}", db_url);
    let is_sqlite = rungu_core::is_sqlite_url(&db_url);

    let config = config::Config::from_env();
    info!("Auth providers: {} active", config.auth.active_providers().len());

    match cli.command {
        Some(Commands::Serve { listen }) => {
            server::serve(config, pool, is_sqlite, &listen).await?;
        }
        Some(Commands::ProjectList) => {
            let store = rungu_core::Store::new_with_kind(pool, is_sqlite);
            let projects = store.list_projects().await?;
            if projects.is_empty() {
                println!("No projects found.");
            } else {
                for p in &projects {
                    println!("  {} ({})", p.name, p.slug);
                }
            }
        }
        Some(Commands::ProjectAdd { name, slug, description }) => {
            let store = rungu_core::Store::new_with_kind(pool, is_sqlite);
            let slug = slug.unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let project = store.create_project(&name, &slug, &description).await?;
            println!("Created project: {} ({})", project.name, project.slug);
        }
        Some(Commands::Healthcheck) => {
            let store = rungu_core::Store::new_with_kind(pool, is_sqlite);
            let _ = store.list_projects().await?;
            println!("OK");
        }
        Some(Commands::Mcp) => {
            // Same operations + subscribers as the HTTP server, so mutations
            // made by an agent fire webhooks and analytics too.
            let store = rungu_core::Store::new_with_kind(pool, is_sqlite);
            let events = rungu_api::default_event_bus(
                &store,
                &reqwest::Client::new(),
                &config.auth,
                rungu_api::email::EmailConfig::from_env(),
            );
            // Storage is only needed to delete attachment files. Clients often
            // spawn `rungu mcp` from an unrelated working directory, so a
            // storage failure must not stop the tools from starting.
            let storage: std::sync::Arc<dyn rungu_core::Storage> = match rungu_core::create_storage() {
                Ok(storage) => std::sync::Arc::from(storage),
                Err(e) => {
                    tracing::warn!(
                        "Attachment storage unavailable ({e:#}); attachment files will not be deleted. \
                         Set RUNGU_STORAGE_DIR to the server's upload directory."
                    );
                    std::sync::Arc::new(rungu_core::NoopStorage)
                }
            };
            rungu_mcp::run_server(rungu_core::Operations::new(store, storage, events)).await?;
            // stdin closed: let in-flight webhooks/analytics finish before the
            // runtime is dropped (one delivery with retries stays under ~40s).
            let pending = rungu_api::background::drain(std::time::Duration::from_secs(45)).await;
            if pending > 0 {
                tracing::warn!("{pending} webhook/analytics task(s) still running at exit were cancelled");
            }
        }
        None => {
            server::serve(config, pool, is_sqlite, "0.0.0.0:3000").await?;
        }
    }

    Ok(())
}
