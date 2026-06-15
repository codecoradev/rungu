//! Vote routes — toggle and check vote status on posts.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{VoteStatusResponse, VoteToggleResponse};

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new().route("/posts/{id}/vote", axum::routing::post(toggle_vote).get(check_voted))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// Toggle vote on a post. One user = one vote per post (upvote/unvote).
#[utoipa::path(
    post,
    path = "/api/posts/{id}/vote",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    responses(
        (status = 200, description = "Vote toggled", body = VoteToggleResponse),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "votes",
)]
pub async fn toggle_vote(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let _ = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    let voted = state.store.toggle_vote(&user.id, &id).await?;

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
#[utoipa::path(
    get,
    path = "/api/posts/{id}/vote",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    responses(
        (status = 200, description = "Vote status", body = VoteStatusResponse),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "votes",
)]
pub async fn check_voted(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let _ = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    let voted = state.store.has_voted(&user.id, &id).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "voted": voted,
        }
    })))
}
