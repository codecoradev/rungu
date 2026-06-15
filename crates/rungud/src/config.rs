//! Daemon configuration.

use rungu_auth::AuthConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub db_path: PathBuf,
    pub listen_addr: String,
    pub auth: AuthConfig,
    /// Allowed CORS origins. Empty = allow all (development only).
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// Set to false to disable Secure cookie flag (e.g. for local HTTP development).
    #[serde(default = "default_secure_cookie")]
    pub secure_cookie: bool,
}

fn default_secure_cookie() -> bool {
    true
}

impl Config {
    /// Build config from environment variables.
    pub fn from_env() -> Self {
        let db_path = std::env::var("RUNGU_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("rungu.db"));

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
            secure_cookie: std::env::var("RUNGU_SECURE_COOKIE")
                .map(|v| v != "false")
                .unwrap_or(true),
        }
    }
}
