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

// ── OAuth redirect target token ───────────────────────────────────────

/// JWT claims for the signed post-login redirect target cookie.
///
/// OAuth providers don't echo arbitrary custom params back to the callback, so
/// we carry the intended post-login destination in a short-lived signed cookie
/// (separate from the CSRF `state` cookie) and verify it in `callback`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RedirectClaims {
    /// Destination path (relative, validated to start with `/` by the caller).
    pub dest: String,
    pub iss: String,
    pub exp: i64,
    pub iat: i64,
}

/// Maximum lifetime of a redirect cookie (5 minutes).
/// Should cover the OAuth round-trip but not outlive a user session.
const REDIRECT_MAX_AGE_SECS: i64 = 300;

/// Issue a signed token wrapping `dest` (a relative path).
///
/// `dest` is **not** re-validated here — callers must ensure it's a safe
/// relative path before signing. Verification (`validate_redirect_token`)
/// re-checks the shape so a tampered token cannot smuggle an open-redirect.
pub fn issue_redirect_token(dest: &str, app_secret: &str) -> Result<String> {
    let now = Utc::now();
    let claims = RedirectClaims {
        dest: dest.to_string(),
        iss: JWT_ISSUER.to_string(),
        exp: now.timestamp() + REDIRECT_MAX_AGE_SECS,
        iat: now.timestamp(),
    };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(app_secret.as_bytes()))
        .context("Failed to issue redirect token")?;
    Ok(token)
}

/// Validate a signed redirect token and return the destination path.
///
/// Rejects expired tokens, tokens signed by a different secret, and tokens whose
/// `dest` is not a safe relative path (must start with `/`, must not start with
/// `//` (protocol-relative), must not contain `\`). This is the second line of
/// defense against open-redirect abuse.
pub fn validate_redirect_token(token: &str, app_secret: &str) -> Result<String> {
    let mut validation = Validation::default();
    validation.set_issuer(&[JWT_ISSUER]);

    let data = decode::<RedirectClaims>(token, &DecodingKey::from_secret(app_secret.as_bytes()), &validation)
        .context("Invalid redirect token")?;

    let dest = data.claims.dest;
    if !is_safe_relative_path(&dest) {
        anyhow::bail!("Unsafe redirect destination: {dest:?}");
    }
    Ok(dest)
}

/// Returns true iff `path` is a safe relative path for post-login redirect.
fn is_safe_relative_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        // Block control chars: CR/LF could header-inject a Location header, and a
        // tab or other control is stripped by the browser's URL parser, so
        // `/<TAB>/evil.example` would become the protocol-relative `//evil.example`.
        && !path.chars().any(char::is_control)
}

#[cfg(test)]
mod redirect_path_tests {
    use super::is_safe_relative_path;

    #[test]
    fn only_plain_same_origin_paths_are_safe() {
        for ok in ["/", "/board/demo", "/board/demo?sort=trending", "/a%20b"] {
            assert!(is_safe_relative_path(ok), "{ok:?}");
        }
        for bad in [
            "board/demo",
            "//evil.example",
            "/\\evil.example",
            "https://evil.example",
            "/\nSet-Cookie: x=1",
            "/\r\n",
            // A tab (or any control character) is stripped by browsers' URL parsers,
            // turning this into the protocol-relative `//evil.example`.
            "/\t/evil.example",
            "/\u{0b}/evil.example",
            "/\u{7f}/x",
        ] {
            assert!(!is_safe_relative_path(bad), "{bad:?}");
        }
    }
}
