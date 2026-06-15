//! Vote routes — toggle and check vote status on posts.
//!
//! Follows the module pattern established by `post_routes.rs`:
//! - `pub fn router() -> Router<AppState>`
//! - Uses `crate::error::ApiError` for all errors
//! - Success: `{ "data": ... }`, Error: `{ "error": "msg" }`

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

/// Build vote routes.
///
/// ```text
/// POST /api/posts/:id/vote — toggle vote (auth required)
/// GET  /api/posts/:id/vote — check voted status (auth required)
/// ```
pub fn router() -> Router<AppState> {
    Router::new().route("/posts/:id/vote", axum::routing::post(toggle_vote).get(check_voted))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// Toggle vote on a post. One user = one vote per post (upvote/unvote).
///
/// Returns `{ "data": { "voted": bool, "vote_count": i64 } }`.
async fn toggle_vote(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    // Verify post exists
    let _ = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    // Toggle vote — returns true if now voted, false if unvoted
    let voted = state.store.toggle_vote(&user.id, &id).await?;

    // Fetch updated post to get current vote_count
    let post = state.store.get_post(&id, Some(&user.id)).await?;

    let vote_count = post.map(|p| p.post.vote_count).unwrap_or(0);

    Ok(Json(serde_json::json!({
        "data": {
            "voted": voted,
            "vote_count": vote_count,
        }
    })))
}

/// Check if the current user has voted on a post.
///
/// Returns `{ "data": { "voted": bool } }`.
async fn check_voted(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    // Verify post exists
    let _ = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    let voted = state.store.has_voted(&user.id, &id).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "voted": voted,
        }
    })))
}
