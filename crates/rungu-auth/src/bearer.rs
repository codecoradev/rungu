//! The seam for issuer-signed Bearer tokens on the REST API.
//!
//! The [`CurrentUser`](crate::CurrentUser) extractors live here, but verifying
//! an access token needs the database and the issuer's keys, which belong to
//! the host crate. The host implements [`BearerVerifier`] and hands it over as
//! [`IssuerAuth`] (extracted from the state with `FromRef`). Without one the
//! extractors behave exactly as before: any Bearer that is not the API key is
//! a 401.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rungu_proto::CurrentUser as CurrentUserData;

/// Why a presented token was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BearerFailure {
    /// The token is not acceptable (bad signature, expired, wrong audience,
    /// unverified or reserved email, ...). The reason is for logs only.
    Invalid(&'static str),
    /// The issuer could not be reached, so the token could not be judged.
    Unavailable,
}

/// The boxed future a [`BearerVerifier`] returns.
pub type VerifyFuture<'a> = Pin<Box<dyn Future<Output = Result<CurrentUserData, BearerFailure>> + Send + 'a>>;

/// Verifies an access token from the configured issuer and resolves the
/// Rungu user behind it. Implemented by the host crate. The identity it
/// returns is the one the request runs as, so the host applies any cap on
/// its rights (REST issuer tokens never carry admin) before returning it.
pub trait BearerVerifier: Send + Sync {
    /// Verify `token` for the REST API and return the user it names.
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a>;

    /// The `WWW-Authenticate` value for a refused token (RFC 6750).
    fn challenge(&self) -> String;
}

/// Issuer-token support, `FromRef`-extractable. `None` = off (the default).
#[derive(Clone, Default)]
pub struct IssuerAuth(pub Option<Arc<dyn BearerVerifier>>);

/// A refused request: the status, and for issuer-token failures a message and
/// a `WWW-Authenticate` challenge. A plain status renders with an empty body,
/// as the extractors always did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRejection {
    status: StatusCode,
    message: Option<&'static str>,
    challenge: Option<String>,
}

impl AuthRejection {
    pub(crate) fn unauthorized() -> Self {
        Self { status: StatusCode::UNAUTHORIZED, message: None, challenge: None }
    }

    pub(crate) fn invalid_token(challenge: String) -> Self {
        Self { status: StatusCode::UNAUTHORIZED, message: Some("Invalid access token"), challenge: Some(challenge) }
    }

    pub(crate) fn unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: Some("Could not verify the access token right now"),
            challenge: None,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn challenge(&self) -> Option<&str> {
        self.challenge.as_deref()
    }
}

impl PartialEq<StatusCode> for AuthRejection {
    fn eq(&self, other: &StatusCode) -> bool {
        self.status == *other
    }
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        let mut response = match self.message {
            Some(message) => (self.status, Json(serde_json::json!({ "error": message }))).into_response(),
            None => self.status.into_response(),
        };
        if let Some(value) = self.challenge.as_deref().and_then(|c| HeaderValue::from_str(c).ok()) {
            response.headers_mut().insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}
