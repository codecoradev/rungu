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
    pub admin_emails: Vec<String>,
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
            app_secret: std::env::var("APP_SECRET").unwrap_or_else(|_| {
                eprintln!("FATAL: APP_SECRET environment variable is not set. Generate one with: openssl rand -hex 32");
                std::process::exit(1);
            }),
            secure_cookie: parse_bool_env("RUNGU_SECURE_COOKIE", true),
            admin_emails: env::var("ADMIN_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            app_url,
            google,
            github,
            keycloak,
        }
    }

    fn build_google(app_url: &str) -> Option<ProviderConfig> {
        let client_id = std::env::var("GOOGLE_CLIENT_ID").ok().filter(|s| !s.is_empty())?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok().filter(|s| !s.is_empty())?;
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
        let client_id = std::env::var("GITHUB_CLIENT_ID").ok().filter(|s| !s.is_empty())?;
        let client_secret = std::env::var("GITHUB_CLIENT_SECRET").ok().filter(|s| !s.is_empty())?;
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
        let base_url = std::env::var("KEYCLOAK_URL").ok().filter(|s| !s.is_empty())?;
        let realm = std::env::var("KEYCLOAK_REALM").ok().filter(|s| !s.is_empty())?;
        let client_id = std::env::var("KEYCLOAK_CLIENT_ID").ok().filter(|s| !s.is_empty())?;
        let client_secret = std::env::var("KEYCLOAK_CLIENT_SECRET").ok().filter(|s| !s.is_empty())?;
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

/// Parse a boolean env var strictly.
///
/// Accepts (case-insensitive): `true`, `1`, `yes`, `y`, `on` → `true`
///                            `false`, `0`, `no`, `n`, `off` → `false`
/// Empty string → falls back to `default_value`.
/// Any other value → logs a fatal error and exits, rather than silently picking
/// a side. This prevents the old `v != "false"` footgun where typos like
/// `False`, `0`, or `no` silently enabled secure cookies and broke local HTTP login.
fn parse_bool_env(name: &str, default_value: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let lower = v.trim().to_lowercase();
            match lower.as_str() {
                "" => default_value,
                "true" | "1" | "yes" | "y" | "on" => true,
                "false" | "0" | "no" | "n" | "off" => false,
                other => {
                    eprintln!(
                        "FATAL: Invalid boolean value for {name}: {other:?}. \
                         Accepted (case-insensitive): true|false|1|0|yes|no|on|off. \
                         Example: {name}=false"
                    );
                    std::process::exit(1);
                }
            }
        }
        Err(_) => default_value,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_bool_env;
    use std::sync::Mutex;

    // ENV manipulation is process-global, so serialize the tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_env(name: &str, value: Option<&str>) {
        // SAFETY: tests are serialized via ENV_LOCK, so there are no concurrent
        // env mutations from other tests in this binary. The env keys used
        // (RUNGU_TEST_BOOL_*) are test-only and never read by production code
        // outside the single parse_bool_env call under test.
        match value {
            Some(v) => unsafe { std::env::set_var(name, v) },
            None => unsafe { std::env::remove_var(name) },
        }
    }

    #[test]
    fn parse_bool_env_unset_returns_default() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "RUNGU_TEST_BOOL_UNSET";
        set_env(name, None);
        assert!(parse_bool_env(name, true));
        assert!(!parse_bool_env(name, false));
    }

    #[test]
    fn parse_bool_env_empty_returns_default() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "RUNGU_TEST_BOOL_EMPTY";
        set_env(name, Some(""));
        assert!(parse_bool_env(name, true));
        assert!(!parse_bool_env(name, false));
    }

    #[test]
    fn parse_bool_env_truthy_values() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "RUNGU_TEST_BOOL_TRUE";
        for v in ["true", "TRUE", "True", "1", "yes", "YES", "y", "Y", "on", "ON"] {
            set_env(name, Some(v));
            assert!(parse_bool_env(name, false), "expected truthy for {v:?}");
        }
    }

    #[test]
    fn parse_bool_env_falsy_values() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "RUNGU_TEST_BOOL_FALSE";
        for v in ["false", "FALSE", "False", "0", "no", "NO", "n", "off", "OFF"] {
            set_env(name, Some(v));
            assert!(!parse_bool_env(name, true), "expected falsy for {v:?}");
        }
    }
}
