//! Auth configuration — ENV-driven provider setup.

use rungu_proto::{AuthProvider, ProviderInfo};
use std::env;

/// OAuth provider configuration.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: Option<String>,
}

/// Full auth configuration parsed from ENV.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub app_secret: String,
    pub app_url: String,
    pub secure_cookie: bool,
    pub google: Option<ProviderConfig>,
    pub github: Option<ProviderConfig>,
    pub keycloak: Option<ProviderConfig>,
}

impl AuthConfig {
    /// Build config from environment variables.
    pub fn from_env() -> Self {
        let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let google = Self::build_google(&app_url);
        let github = Self::build_github(&app_url);
        let keycloak = Self::build_keycloak(&app_url);

        Self {
            app_secret: env::var("APP_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            secure_cookie: env::var("RUNGU_SECURE_COOKIE").map(|v| v != "false").unwrap_or(true),
            app_url,
            google,
            github,
            keycloak,
        }
    }

    fn build_google(app_url: &str) -> Option<ProviderConfig> {
        let client_id = env::var("GOOGLE_CLIENT_ID").ok()?;
        let client_secret = env::var("GOOGLE_CLIENT_SECRET").ok()?;
        Some(ProviderConfig {
            client_id: client_id.clone(),
            client_secret,
            redirect_uri: format!("{}/auth/google/callback", app_url),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_url: Some("https://www.googleapis.com/oauth2/v2/userinfo".to_string()),
        })
    }

    fn build_github(app_url: &str) -> Option<ProviderConfig> {
        let client_id = env::var("GITHUB_CLIENT_ID").ok()?;
        let client_secret = env::var("GITHUB_CLIENT_SECRET").ok()?;
        Some(ProviderConfig {
            client_id: client_id.clone(),
            client_secret,
            redirect_uri: format!("{}/auth/github/callback", app_url),
            auth_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            userinfo_url: Some("https://api.github.com/user".to_string()),
        })
    }

    fn build_keycloak(app_url: &str) -> Option<ProviderConfig> {
        let base_url = env::var("KEYCLOAK_URL").ok()?;
        let realm = env::var("KEYCLOAK_REALM").ok()?;
        let client_id = env::var("KEYCLOAK_CLIENT_ID").ok()?;
        let client_secret = env::var("KEYCLOAK_CLIENT_SECRET").ok()?;
        let realm_url = format!("{}/realms/{}", base_url, realm);
        Some(ProviderConfig {
            client_id,
            client_secret,
            redirect_uri: format!("{}/auth/keycloak/callback", app_url),
            auth_url: format!("{}/protocol/openid-connect/auth", realm_url),
            token_url: format!("{}/protocol/openid-connect/token", realm_url),
            userinfo_url: Some(format!("{}/protocol/openid-connect/userinfo", realm_url)),
        })
    }

    /// Get list of active providers for frontend login buttons.
    pub fn active_providers(&self) -> Vec<ProviderInfo> {
        let mut providers = Vec::new();
        if self.google.is_some() {
            providers.push(ProviderInfo {
                name: "google".to_string(),
                login_url: format!("{}/auth/google/login", self.app_url),
            });
        }
        if self.github.is_some() {
            providers.push(ProviderInfo {
                name: "github".to_string(),
                login_url: format!("{}/auth/github/login", self.app_url),
            });
        }
        if self.keycloak.is_some() {
            providers.push(ProviderInfo {
                name: "keycloak".to_string(),
                login_url: format!("{}/auth/keycloak/login", self.app_url),
            });
        }
        providers
    }

    /// Get provider config by name.
    pub fn get_provider(&self, name: &str) -> Option<(&ProviderConfig, AuthProvider)> {
        match name {
            "google" => self.google.as_ref().map(|c| (c, AuthProvider::Google)),
            "github" => self.github.as_ref().map(|c| (c, AuthProvider::GitHub)),
            "keycloak" => self.keycloak.as_ref().map(|c| (c, AuthProvider::Keycloak)),
            _ => None,
        }
    }
}
