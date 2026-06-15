//! # rungu-auth
//!
//! Multi-provider OAuth (Google/GitHub/Keycloak) + JWT session + Axum middleware.
//! ENV-driven — empty provider config = disabled.

pub mod config;
pub mod middleware;
pub mod session;

pub use config::AuthConfig;
pub use middleware::CurrentUser;
