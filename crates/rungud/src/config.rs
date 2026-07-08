/// Daemon configuration.
use rungu_auth::AuthConfig;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub listen_addr: String,
    pub auth: AuthConfig,
    /// Allowed CORS origins. Empty = allow all (development only).
    pub cors_origins: Vec<String>,
    /// Sentry DSN. None = Sentry disabled (no events sent).
    pub sentry_dsn: Option<String>,
    /// Max requests per minute per IP on `/api/*` routes. `0` disables the
    /// API rate limiter entirely.
    pub rate_limit_per_min: u32,
    /// Max requests per minute per IP on `/auth/*` routes (OAuth
    /// login/callback). Stricter than the API limiter to blunt credential /
    /// OAuth abuse. `0` disables it.
    pub auth_rate_limit_per_min: u32,
    /// Honor `X-Forwarded-For` when resolving client IPs for rate limiting.
    /// Default `false` (use the socket address) so a directly-exposed Rungu
    /// can't be spoofed. Enable only behind a trusted reverse proxy that
    /// **overwrites** the header.
    pub trust_proxy: bool,
}

impl Config {
    /// Build config from environment variables.
    pub fn from_env() -> Self {
        let db_path =
            std::env::var("RUNGU_DB").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(Self::default_db_path()));

        let cors_origins = std::env::var("RUNGU_CORS_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            db_path,
            listen_addr: std::env::var("RUNGU_LISTEN").unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            auth: AuthConfig::from_env(),
            cors_origins,
            sentry_dsn: std::env::var("SENTRY_DSN").ok(),
            rate_limit_per_min: parse_per_min("RUNGU_RATE_LIMIT_PER_MIN", 300),
            auth_rate_limit_per_min: parse_per_min("RUNGU_AUTH_RATE_LIMIT_PER_MIN", 30),
            trust_proxy: parse_bool("RUNGU_TRUST_PROXY", false),
        }
    }

    /// Returns the default data directory: `~/.codecora/rungu/`.
    ///
    /// Override with `RUNGU_DATA_DIR` env var.
    pub fn default_data_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("RUNGU_DATA_DIR") {
            return PathBuf::from(dir);
        }
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".codecora").join("rungu")
    }

    /// Returns the default database path: `~/.codecora/rungu/rungu.db`.
    pub fn default_db_path() -> String {
        Self::default_data_dir().join("rungu.db").to_string_lossy().to_string()
    }

    /// Ensure the data directory exists, creating it if necessary.
    pub fn ensure_data_dir(dir: &PathBuf) -> std::io::Result<()> {
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
            tracing::info!("Created data directory: {}", dir.display());
        }
        Ok(())
    }
}

/// Parse a `requests per minute` env var into a `u32`. Falls back to
/// `default` when unset or unparseable. `0` is a valid value meaning
/// "disable this limiter".
fn parse_per_min(var: &str, default: u32) -> u32 {
    std::env::var(var).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

/// Parse a boolean env var. Accepts true/1/yes/on (case-insensitive);
/// everything else (including unset) falls back to `default`.
fn parse_bool(var: &str, default: bool) -> bool {
    match std::env::var(var).ok().and_then(|s| s.to_lowercase().parse().ok()) {
        Some(v) => v,
        None => default,
    }
}
