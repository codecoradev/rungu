//! Post routes — CRUD for feedback posts.
//!
//! Pattern: each route module exposes a `router()` function that returns
//! `Router<AppState>`. Handlers use `CurrentUser` for auth, `State<AppState>`
//! for DB access, and `Path`/`Query` for params.
//!
//! Response envelope: `{ "data": T }` on success, `{ "error": "msg" }` on failure.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{PostCategory, PostSort, PostStatus};
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;

// ── Request types ──────────────────────────────────────────────────────

/// Query params for `GET /api/projects/:slug/posts`.
#[derive(Debug, Deserialize)]
pub struct ListPostsQuery {
    pub sort: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub q: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Body for `POST /api/projects/:slug/posts`.
#[derive(Debug, Deserialize)]
pub struct CreatePostBody {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
}

/// Body for `PATCH /api/posts/:id`.
#[derive(Debug, Deserialize)]
pub struct UpdatePostBody {
    pub status: Option<String>,
}

impl UpdatePostBody {
    /// Check if at least one field is provided.
    fn has_updates(&self) -> bool {
        self.status.is_some()
    }
}

// ── Routes ─────────────────────────────────────────────────────────────

/// Build post routes.
///
/// ```text
/// GET    /api/projects/:slug/posts     — list (public)
/// POST   /api/projects/:slug/posts     — create (auth required)
/// GET    /api/posts/:id                — get detail (public)
/// PATCH  /api/posts/:id                — update status (auth, own post or admin)
/// DELETE /api/posts/:id                — delete (auth, own post or admin)
/// ```
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/:slug/posts", axum::routing::get(list_posts).post(create_post))
        .route("/posts/:id", axum::routing::get(get_post).patch(update_post).delete(delete_post))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List posts for a project with optional filters.
async fn list_posts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListPostsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let params = rungu_proto::ListPostsParams {
        project_id: &project.id,
        sort: parse_sort(query.sort.as_deref()),
        status: query.status.as_deref().and_then(parse_status),
        category: query.category.as_deref().and_then(parse_category),
        query: query.q.as_deref(),
        offset,
        limit: per_page,
    };

    let (posts, total) = state.store.list_posts(params).await?;

    Ok(Json(serde_json::json!({
        "data": posts,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        }
    })))
}

/// Create a new post in a project.
async fn create_post(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreatePostBody>,
) -> Result<(StatusCode, impl IntoResponse), ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let title = body.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("Title is required"));
    }
    if title.len() > 200 {
        return Err(ApiError::bad_request("Title must be 200 characters or less"));
    }

    let category = body.category.as_deref().and_then(parse_category).unwrap_or_default();

    let post = state
        .store
        .create_post(&project.id, title, body.description.unwrap_or_default().as_str(), category, &user.id)
        .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": post }))))
}

/// Get a single post with detail.
async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: rungu_auth::OptionalCurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = user.user.as_ref().map(|cu| cu.id.as_str());

    let post = state.store.get_post(&id, user_id).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    Ok(Json(serde_json::json!({ "data": post })))
}

/// Update a post's status (author or admin only).
async fn update_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdatePostBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !body.has_updates() {
        return Err(ApiError::bad_request("No fields to update"));
    }

    // Fetch existing post to check ownership
    let existing = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    // Only author or admin can update
    let is_author = existing.post.created_by == user.id;
    let is_admin = user.role == rungu_proto::UserRole::Admin;
    if !is_author && !is_admin {
        return Err(ApiError::forbidden("You can only update your own posts"));
    }

    // Update status if provided
    if let Some(status_str) = &body.status {
        let status = parse_status(status_str).ok_or_else(|| ApiError::bad_request("Invalid status"))?;
        state.store.update_post_status(&id, status).await?;
    }

    // Return updated post
    let updated = state.store.get_post(&id, Some(&user.id)).await?;

    Ok(Json(serde_json::json!({ "data": updated })))
}

/// Delete a post (author or admin only).
async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    let existing = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    let is_author = existing.post.created_by == user.id;
    let is_admin = user.role == rungu_proto::UserRole::Admin;
    if !is_author && !is_admin {
        return Err(ApiError::forbidden("You can only delete your own posts"));
    }

    state.store.delete_post(&id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Parsing helpers ────────────────────────────────────────────────────

fn parse_sort(s: Option<&str>) -> PostSort {
    match s {
        Some("oldest") => PostSort::Oldest,
        Some("most_votes") => PostSort::MostVotes,
        Some("least_votes") => PostSort::LeastVotes,
        Some("recently_updated") => PostSort::RecentlyUpdated,
        _ => PostSort::Newest,
    }
}

fn parse_status(s: &str) -> Option<PostStatus> {
    match s {
        "open" => Some(PostStatus::Open),
        "planned" => Some(PostStatus::Planned),
        "in_progress" => Some(PostStatus::InProgress),
        "done" => Some(PostStatus::Done),
        "declined" => Some(PostStatus::Declined),
        _ => None,
    }
}

fn parse_category(s: &str) -> Option<PostCategory> {
    match s {
        "feedback" => Some(PostCategory::Feedback),
        "bug" => Some(PostCategory::Bug),
        "feature" => Some(PostCategory::Feature),
        "question" => Some(PostCategory::Question),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sort() {
        assert!(matches!(parse_sort(None), PostSort::Newest));
        assert!(matches!(parse_sort(Some("oldest")), PostSort::Oldest));
        assert!(matches!(parse_sort(Some("most_votes")), PostSort::MostVotes));
        assert!(matches!(parse_sort(Some("unknown")), PostSort::Newest));
    }

    #[test]
    fn test_parse_status() {
        assert!(matches!(parse_status("open"), Some(PostStatus::Open)));
        assert!(matches!(parse_status("done"), Some(PostStatus::Done)));
        assert!(parse_status("invalid").is_none());
    }

    #[test]
    fn test_parse_category() {
        assert!(matches!(parse_category("bug"), Some(PostCategory::Bug)));
        assert!(matches!(parse_category("feature"), Some(PostCategory::Feature)));
        assert!(parse_category("invalid").is_none());
    }

    #[test]
    fn test_update_body_has_updates() {
        assert!(UpdatePostBody { status: Some("done".into()) }.has_updates());
        assert!(!UpdatePostBody { status: None }.has_updates());
    }
}
