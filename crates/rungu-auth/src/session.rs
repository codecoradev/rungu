//! JWT session management.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rungu_proto::CurrentUser;

/// JWT issuer identifier.
const JWT_ISSUER: &str = "rungu";

/// JWT claims for session tokens.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub email: String,
    pub role: String, // "admin" or "member"
    pub iss: String,  // issuer
    pub exp: i64,     // expiry timestamp
    pub iat: i64,     // issued at
}

/// Issue a JWT session token (7-day expiry).
pub fn issue_jwt(user: &CurrentUser, app_secret: &str) -> Result<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: format!("{:?}", user.role).to_lowercase(),
        iss: JWT_ISSUER.to_string(),
        exp: (now + Duration::days(7)).timestamp(),
        iat: now.timestamp(),
    };

    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(app_secret.as_bytes()))
        .context("Failed to issue JWT")?;

    Ok(token)
}

/// Validate a JWT and extract current user.
/// Validates issuer claim to reject tokens from other applications.
pub fn validate_jwt(token: &str, app_secret: &str) -> Result<CurrentUser> {
    let mut validation = Validation::default();
    validation.set_issuer(&[JWT_ISSUER]);

    let data = decode::<Claims>(token, &DecodingKey::from_secret(app_secret.as_bytes()), &validation)
        .context("Invalid JWT")?;

    let claims = data.claims;
    Ok(CurrentUser {
        id: claims.sub,
        email: claims.email,
        role: match claims.role.as_str() {
            "admin" => rungu_proto::UserRole::Admin,
            _ => rungu_proto::UserRole::Member,
        },
    })
}
