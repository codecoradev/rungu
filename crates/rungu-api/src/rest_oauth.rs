//! Issuer-signed Bearer tokens on the REST API (#272).
//!
//! Implements the `rungu-auth` seam on top of the one [`McpOAuth`] verifier
//! (same keys, discovery and actor caches as `/mcp`), asking for the REST
//! audience instead of the `/mcp` one.
//!
//! The identity a request runs as is capped at `member`: the token is passed
//! through product backends and is not bound to a project, so it never carries
//! admin rights, even when the email is in `ADMIN_EMAILS`. The stored role is
//! not touched. Admin actions need the web session or `RUNGU_API_KEY`.

use std::sync::Arc;

use rungu_auth::{BearerFailure, BearerVerifier, IssuerAuth, bearer::VerifyFuture};
use rungu_core::Store;
use rungu_proto::{CurrentUser, UserRole};

use crate::mcp_oauth::{McpOAuth, Reject};

/// REST token verification backed by the shared issuer verifier.
pub struct RestBearer {
    oauth: Arc<McpOAuth>,
    store: Store,
}

impl RestBearer {
    /// `Some` only when REST issuer auth is configured (issuer and REST resource set).
    pub fn auth(oauth: Option<&Arc<McpOAuth>>, store: &Store) -> IssuerAuth {
        IssuerAuth(
            oauth
                .filter(|o| o.api_resource().is_some())
                .map(|o| Arc::new(Self { oauth: o.clone(), store: store.clone() }) as Arc<dyn BearerVerifier>),
        )
    }
}

impl BearerVerifier for RestBearer {
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a> {
        Box::pin(async move {
            match self.oauth.authenticate_api(token, &self.store).await {
                // Capped: whatever the stored role, this request is a member.
                Ok((actor, email)) => Ok(CurrentUser { id: actor.id, email, role: UserRole::Member }),
                Err(Reject::IssuerUnavailable) => Err(BearerFailure::Unavailable),
                Err(reason) => Err(BearerFailure::Invalid(reason.as_str())),
            }
        })
    }

    fn challenge(&self) -> String {
        self.oauth.api_challenge()
    }
}
