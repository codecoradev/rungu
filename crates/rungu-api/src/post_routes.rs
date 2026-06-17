//! Post routes — CRUD for feedback posts.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{CreatePostBody, PostCategory, PostDetail, PostSort, PostStatus, UpdatePostBody};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::AppState;
use crate::error::ApiError;

// ── Request types ──────────────────────────────────────────────────────

/// Query params for `GET /api/projects/{slug}/posts`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListPostsQuery {
    pub sort: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    /// Search query string.
    pub q: Option<String>,
    /// Page number (1-based).
    pub page: Option<i64>,
    /// Items per page (1-100).
    pub per_page: Option<i64>,
}

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/posts", axum::routing::get(list_posts).post(create_post))
        .route("/projects/{slug}/roadmap", axum::routing::get(get_project_roadmap))
        .route("/projects/{slug}/changelog", axum::routing::get(get_project_changelog))
        .route("/posts/{id}", axum::routing::get(get_post).patch(update_post).delete(delete_post))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List posts for a project with optional filters.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/posts",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ListPostsQuery,
    ),
    responses(
        (status = 200, description = "List of posts with pagination", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn list_posts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListPostsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    // Validate filter values explicitly rather than silently dropping unknown
    // values — otherwise a typo like `?status=opennnn` returns 200 with unfiltered
    // results and hides client bugs. Mirrors update_post_status behavior.
    let status = match query.status.as_deref() {
        Some(s) => Some(parse_status(s).ok_or_else(|| ApiError::bad_request("Invalid status filter"))?),
        None => None,
    };
    let category = match query.category.as_deref() {
        Some(c) => Some(parse_category(c).ok_or_else(|| ApiError::bad_request("Invalid category filter"))?),
        None => None,
    };

    let params = rungu_proto::ListPostsParams {
        project_id: &project.id,
        sort: parse_sort(query.sort.as_deref()),
        status,
        category,
        query: query.q.as_deref(),
        since: None,
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
            "total_pages": ((total as f64 / per_page as f64).ceil() as i64).max(1),
        }
    })))
}

/// Create a new post in a project.
#[utoipa::path(
    post,
    path = "/api/projects/{slug}/posts",
    params(
        ("slug" = String, Path, description = "Project slug"),
    ),
    request_body = CreatePostBody,
    responses(
        (status = 201, description = "Post created", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "posts",
)]
pub async fn create_post(
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
#[utoipa::path(
    get,
    path = "/api/posts/{id}",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    responses(
        (status = 200, description = "Post detail", body = PostDetail),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: rungu_auth::OptionalCurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = user.user.as_ref().map(|cu| cu.id.as_str());

    let post = state.store.get_post(&id, user_id).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    Ok(Json(serde_json::json!({ "data": post })))
}

/// Update a post's status (author or admin only).
#[utoipa::path(
    patch,
    path = "/api/posts/{id}",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    request_body = UpdatePostBody,
    responses(
        (status = 200, description = "Post updated", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Forbidden", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "posts",
)]
pub async fn update_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdatePostBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !body.has_updates() {
        return Err(ApiError::bad_request("No fields to update"));
    }

    let existing = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    ApiError::check_owner_or_admin(&user, &existing.post.created_by, "You can only update your own posts")?;

    if let Some(status_str) = &body.status {
        let status = parse_status(status_str).ok_or_else(|| ApiError::bad_request("Invalid status"))?;
        state.store.update_post_status(&id, status).await?;
    }

    if let Some(category_str) = &body.category {
        let category = parse_category(category_str).ok_or_else(|| ApiError::bad_request("Invalid category"))?;
        state.store.update_post_category(&id, category).await?;
    }

    let updated = state
        .store
        .get_post(&id, Some(&user.id))
        .await?
        .ok_or_else(|| ApiError::internal("Post disappeared after update"))?;

    Ok(Json(serde_json::json!({ "data": updated })))
}

/// Delete a post (author or admin only).
#[utoipa::path(
    delete,
    path = "/api/posts/{id}",
    params(
        ("id" = String, Path, description = "Post ID"),
    ),
    responses(
        (status = 204, description = "Post deleted"),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Forbidden", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "posts",
)]
pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    let existing = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;

    ApiError::check_owner_or_admin(&user, &existing.post.created_by, "You can only delete your own posts")?;

    state.store.delete_post(&id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Roadmap ────────────────────────────────────────────────────────────

/// Query params for `GET /api/projects/{slug}/roadmap`.
///
/// `limit` caps the number of posts returned per status bucket (default 10,
/// max 50) so a project with hundreds of `done` items doesn't flood the board.
#[derive(Debug, Deserialize, IntoParams)]
pub struct RoadmapQuery {
    /// Max posts per status bucket. Defaults to 10, clamped to 1..=50.
    pub limit: Option<i64>,
}

/// Public roadmap view — posts grouped by lifecycle status.
///
/// Returns three buckets (`planned`, `in_progress`, `done`) with posts sorted
/// by vote count (desc) within each bucket. Only posts that have progressed
/// past `open` appear here — `open` and `declined` are intentionally excluded
/// because they don't represent committed work.
///
/// Public (no auth) — mirrors the visibility of `GET /projects/{slug}/posts`.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/roadmap",
    params(
        ("slug" = String, Path, description = "Project slug"),
        RoadmapQuery,
    ),
    responses(
        (status = 200, description = "Posts grouped by status", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_project_roadmap(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<RoadmapQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    // Clamp per-bucket limit. 10 keeps the board readable; 50 is the ceiling
    // so a single API call can never return more than 150 post rows.
    let per_bucket = query.limit.unwrap_or(10).clamp(1, 50);

    // Fetch each committed-work bucket. We reuse `list_posts` so the roadmap
    // inherits its parameter binding, sort, and (FTS5/LIKE) search behavior.
    // Three round-trips is acceptable — these are indexed status scans, and
    // it keeps the store API the single source of truth for post queries.
    let planned = fetch_bucket(&state, &project.id, PostStatus::Planned, per_bucket).await?;
    let in_progress = fetch_bucket(&state, &project.id, PostStatus::InProgress, per_bucket).await?;
    let done = fetch_bucket(&state, &project.id, PostStatus::Done, per_bucket).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "planned": planned.0,
            "planned_total": planned.1,
            "in_progress": in_progress.0,
            "in_progress_total": in_progress.1,
            "done": done.0,
            "done_total": done.1,
            "limit": per_bucket,
        }
    })))
}

/// Fetch a single roadmap bucket: the top-N most-voted posts for `status`.
///
/// Returns `(posts, total_matching)` so the UI can show "12 planned" even when
/// only the top 10 are rendered.
async fn fetch_bucket(
    state: &AppState,
    project_id: &str,
    status: PostStatus,
    limit: i64,
) -> Result<(Vec<PostDetail>, i64), ApiError> {
    let params = rungu_proto::ListPostsParams {
        project_id,
        sort: PostSort::MostVotes,
        status: Some(status),
        category: None,
        query: None,
        since: None,
        offset: 0,
        limit,
    };
    let (posts, total) = state.store.list_posts(params).await?;
    Ok((posts, total))
}

// ── Changelog ──────────────────────────────────────────────────────────

/// Query params for `GET /api/projects/{slug}/changelog`.
///
/// Mirrors the board's pagination plus an optional `since` lower bound on
/// `updated_at` (RFC3339) for incremental pulls ("what shipped since my last
/// sync?").
#[derive(Debug, Deserialize, IntoParams)]
pub struct ChangelogQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    /// RFC3339 timestamp; only posts updated at or after this time are returned.
    pub since: Option<String>,
}

/// Public changelog — `done` posts sorted by most-recently-shipped first.
///
/// Returns posts with `status == done` ordered by `updated_at DESC`. Supports
/// the same pagination shape as the board list, plus an optional `?since=`
/// filter for incremental syncs. Public (no auth).
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/changelog",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ChangelogQuery,
    ),
    responses(
        (status = 200, description = "Done posts, newest ship first", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_project_changelog(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ChangelogQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    // Parse the optional `since` bound. A malformed timestamp must 400, not
    // silently behave as "no filter" — otherwise a client bug (wrong timezone
    // format, stray space) would silently return the full history.
    let since = match query.since.as_deref() {
        Some(s) => Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|_| ApiError::bad_request("Invalid `since` timestamp; expected RFC3339"))?,
        ),
        None => None,
    };

    let params = rungu_proto::ListPostsParams {
        project_id: &project.id,
        sort: PostSort::RecentlyUpdated,
        status: Some(PostStatus::Done),
        category: None,
        query: None,
        since,
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
            "total_pages": ((total as f64 / per_page as f64).ceil() as i64).max(1),
        }
    })))
}

// ── Parsing helpers ────────────────────────────────────────────────────

pub(crate) fn parse_sort(s: Option<&str>) -> PostSort {
    match s {
        Some("oldest") => PostSort::Oldest,
        Some("most_votes") => PostSort::MostVotes,
        Some("least_votes") => PostSort::LeastVotes,
        Some("recently_updated") => PostSort::RecentlyUpdated,
        _ => PostSort::Newest,
    }
}

pub(crate) fn parse_status(s: &str) -> Option<PostStatus> {
    match s {
        "open" => Some(PostStatus::Open),
        "planned" => Some(PostStatus::Planned),
        "in_progress" => Some(PostStatus::InProgress),
        "done" => Some(PostStatus::Done),
        "declined" => Some(PostStatus::Declined),
        _ => None,
    }
}

pub(crate) fn parse_category(s: &str) -> Option<PostCategory> {
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
        assert!(UpdatePostBody { status: Some("done".into()), category: None }.has_updates());
        assert!(!UpdatePostBody { status: None, category: None }.has_updates());
    }
}
