//! Comment routes — create, list, delete (threaded via parent_id).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::CreateCommentBody;

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/posts/{id}/comments", axum::routing::get(list_comments).post(create_comment))
        .route("/comments/{id}", axum::routing::delete(delete_comment))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List all comments for a post (oldest-first for threading).
#[utoipa::path(
    get,
    path = "/api/posts/{id}/comments",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    responses(
        (status = 200, description = "List of comments", body = serde_json::Value),
    ),
    tag = "comments",
)]
pub async fn list_comments(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let comments = state.store.list_comments(&post_id).await?;
    Ok(Json(serde_json::json!({ "data": comments })))
}

/// Create a new comment on a post (supports threading via `parent_id`).
#[utoipa::path(
    post,
    path = "/api/posts/{id}/comments",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    request_body = CreateCommentBody,
    responses(
        (status = 201, description = "Comment created", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "comments",
)]
pub async fn create_comment(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreateCommentBody>,
) -> Result<(StatusCode, impl IntoResponse), ApiError> {
    let content = body.content.trim();
    if content.is_empty() {
        return Err(ApiError::bad_request("Comment content is required"));
    }
    if content.len() > 4000 {
        return Err(ApiError::bad_request("Comment must be 4000 characters or less"));
    }

    let _post = state.store.get_post(&post_id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    if let Some(ref parent_id) = body.parent_id {
        let parent = state
            .store
            .get_comment(parent_id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Parent comment not found"))?;
        if parent.post_id != post_id {
            return Err(ApiError::bad_request("Parent comment does not belong to this post"));
        }
    }

    let comment = state.store.create_comment(&post_id, content, body.parent_id.as_deref(), &user.id).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": comment }))))
}

/// Delete a comment (author or admin only).
#[utoipa::path(
    delete,
    path = "/api/comments/{id}",
    params(
        ("id" = String, Path, description = "Comment ID"),
    ),
    responses(
        (status = 204, description = "Comment deleted"),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Forbidden", body = serde_json::Value),
        (status = 404, description = "Comment not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "comments",
)]
pub async fn delete_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    let existing =
        state.store.get_comment(&comment_id).await?.ok_or_else(|| ApiError::not_found("Comment not found"))?;

    let is_author = existing.created_by == user.id;
    let is_admin = user.role == rungu_proto::UserRole::Admin;
    if !is_author && !is_admin {
        return Err(ApiError::forbidden("You can only delete your own comments"));
    }

    state.store.delete_comment(&comment_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use rungu_proto::CreateCommentBody;

    #[test]
    fn test_create_body_validation() {
        let body = CreateCommentBody { content: "  ".to_string(), parent_id: None };
        assert!(body.content.trim().is_empty());

        let body = CreateCommentBody { content: "Great idea!".to_string(), parent_id: None };
        assert!(!body.content.trim().is_empty());

        let body = CreateCommentBody { content: "Reply".to_string(), parent_id: Some("parent-uuid".to_string()) };
        assert!(body.parent_id.is_some());
    }
}
