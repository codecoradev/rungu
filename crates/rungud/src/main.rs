//! # rungud — Rungu daemon
//!
//! Main binary: CLI subcommands + HTTP server.

pub mod config;
pub mod server;
pub mod spa;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "rungu", version, about = "Rungu — lightweight feedback board daemon")]
struct Cli {
    /// Database path (SQLite)
    #[arg(short, long, global = true, default_value = "rungu.db")]
    db: PathBuf,

    /// Log level
    #[arg(short, long, global = true, default_value = "info")]
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
        #[arg(short, long, default_value = "")]
        description: String,
    },
    /// Health check (exit 0 if healthy)
    Healthcheck,
    /// Start MCP server (stdio JSON-RPC 2.0)
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .init();

    let pool = rungu_core::open_pool(&format!("sqlite:{}", cli.db.display())).await?;
    rungu_core::run_migrations(&pool).await?;
    info!("Database ready: {}", cli.db.display());

    let config = config::Config::from_env();
    info!("Auth providers: {} active", config.auth.active_providers().len());

    match cli.command {
        Some(Commands::Serve { listen }) => {
            server::serve(config, pool, &listen).await?;
        }
        Some(Commands::ProjectList) => {
            let store = rungu_core::Store::new(pool);
            let projects = store.list_projects().await?;
            if projects.is_empty() {
                println!("No projects found.");
            } else {
                for p in &projects {
                    println!("  {} ({})", p.name, p.slug);
                }
            }
        }
        Some(Commands::ProjectAdd {
            name,
            slug,
            description,
        }) => {
            let store = rungu_core::Store::new(pool);
            let slug = slug.unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let project = store.create_project(&name, &slug, &description).await?;
            println!("Created project: {} ({})", project.name, project.slug);
        }
        Some(Commands::Healthcheck) => {
            // Simple health check — can we query the DB?
            let store = rungu_core::Store::new(pool);
            let _ = store.list_projects().await?;
            println!("OK");
        }
        Some(Commands::Mcp) => {
            rungu_mcp::run_server(pool).await?;
        }
        None => {
            // Default: serve
            server::serve(config, pool, "0.0.0.0:3000").await?;
        }
    }

    Ok(())
}
