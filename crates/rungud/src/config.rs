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
}

impl Config {
    /// Build config from environment variables.
    pub fn from_env() -> Self {
        let db_path = std::env::var("RUNGU_DB").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("rungu.db"));

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
        }
    }
}
