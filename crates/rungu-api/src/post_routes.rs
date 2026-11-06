//! Post routes — CRUD for feedback posts.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_core::Actor;
use rungu_core::ops::{NewPost, PostChanges};
use rungu_core::query::{BoardFilters, PageRequest, PostPage};
use rungu_proto::{CreatePostBody, PostView, UpdatePostBody};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

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
        .route("/posts/{id}/official-response", axum::routing::get(get_official_response).put(set_official_response))
        .route("/posts/{id}/similar", axum::routing::get(get_similar_posts))
        .route("/projects/{slug}/similar", axum::routing::get(get_similar_by_title))
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
    user: rungu_auth::OptionalCurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let filters = BoardFilters {
        sort: query.sort.as_deref(),
        status: query.status.as_deref(),
        category: query.category.as_deref(),
        q: query.q.as_deref(),
    };
    let viewer = user.user.as_ref().map(|cu| cu.id.as_str());
    let (project, posts) =
        state.ops.queries().board(&slug, filters, page_request(query.page, query.per_page), viewer).await?;

    // Analytics: board view (#186) — fire-and-forget, aggregate only.
    crate::analytics::capture(&state.store, &project.id, None, "board_view");

    Ok(Json(paginated(posts)))
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
    let description = body.description.unwrap_or_default();
    let post = state
        .ops
        .create_post(
            &Actor::from(&user),
            &slug,
            NewPost { title: &body.title, description: &description, category: body.category.as_deref() },
        )
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
        (status = 200, description = "Post detail, with what the viewer may do with it", body = PostView),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: rungu_auth::OptionalCurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let viewer = user.user.as_ref().map(Actor::from);

    let post = state.ops.queries().post(&id, viewer.as_ref()).await?;

    // Analytics: post view (#186).
    crate::analytics::capture(&state.store, &post.detail.post.project_id, Some(&post.detail.post.id), "post_view");

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
    // Ownership is checked before body validation inside `ops` (#162).
    let changes = PostChanges { status: body.status.as_deref(), category: body.category.as_deref() };
    let updated = state.ops.update_post(&Actor::from(&user), &id, changes).await?;

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
    state.ops.delete_post(&Actor::from(&user), &id).await?;
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
    let roadmap = state.ops.queries().roadmap(&slug, query.limit).await?;
    Ok(Json(serde_json::json!({
        "data": {
            "planned": roadmap.planned.posts,
            "planned_total": roadmap.planned.total,
            "in_progress": roadmap.in_progress.posts,
            "in_progress_total": roadmap.in_progress.total,
            "done": roadmap.done.posts,
            "done_total": roadmap.done.total,
            "limit": roadmap.limit,
        }
    })))
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
    let posts =
        state.ops.queries().changelog(&slug, query.since.as_deref(), page_request(query.page, query.per_page)).await?;
    Ok(Json(paginated(posts)))
}

// ── Official team response (#205) ──────────────────────────────────────

/// Get the official team response for a post (public).
#[utoipa::path(
    get,
    path = "/api/posts/{id}/official-response",
    responses(
        (status = 200, description = "Official response (or null)", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_official_response(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let post = state.store.get_post(&id, None).await?.ok_or_else(|| ApiError::not_found("Post not found"))?;
    let response = state.store.get_official_response(&post.post.id).await?;
    Ok(Json(serde_json::json!({ "data": response })))
}

/// Set or remove the official team response (admin only) (#205).
/// Authz happens BEFORE body validation — response codes must not leak
/// post existence to non-admins (#162 rule).
#[utoipa::path(
    put,
    path = "/api/posts/{id}/official-response",
    request_body = OfficialResponseBody,
    responses(
        (status = 200, description = "Official response updated", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "posts",
)]
pub async fn set_official_response(
    State(state): State<AppState>,
    Path(id): Path<String>,
    user: CurrentUser,
    body: Result<Json<OfficialResponseBody>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Authn (extractor) + 2. authz — before any body parsing, so the
    // response never reveals post existence to non-admins (#162).
    ApiError::require_admin(&user.0)?;

    // 3. Body — last.
    let Ok(Json(body)) = body else {
        return Err(ApiError::bad_request("Invalid request body"));
    };
    let outcome = state.ops.set_official_response(&Actor::from(&user.0), &id, body.comment_id.as_deref()).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "post_id": id,
            "official_response": outcome.response,
        }
    })))
}

// ── Similar posts (#206) ───────────────────────────────────────────────

/// Public list of similar posts (title-keyword match, max 3) for dedup UX.
#[utoipa::path(
    get,
    path = "/api/posts/{id}/similar",
    responses(
        (status = 200, description = "Up to 3 similar posts", body = serde_json::Value),
        (status = 404, description = "Post not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_similar_posts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let similar = state.ops.queries().similar_to_post(&id).await?;
    Ok(Json(serde_json::json!({ "data": similar })))
}

/// Query for the as-you-type duplicate check.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SimilarQuery {
    /// Draft post title to match against existing posts.
    pub title: String,
}

/// Public list of up to 3 existing posts that look like a draft title, so the
/// New Post form can suggest voting on an existing post instead of filing a
/// duplicate. Uses the same keyword match as `/api/posts/{id}/similar`.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/similar",
    params(SimilarQuery),
    responses(
        (status = 200, description = "Up to 3 posts similar to the draft title", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "posts",
)]
pub async fn get_similar_by_title(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<SimilarQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let similar = state.ops.queries().similar_to_title(&slug, &query.title).await?;
    Ok(Json(serde_json::json!({ "data": similar })))
}

// ── Encoding helpers ───────────────────────────────────────────────────

fn page_request(page: Option<i64>, per_page: Option<i64>) -> PageRequest {
    PageRequest { page, per_page }
}

/// The REST shape of a [`PostPage`]: `{ data, pagination }`.
fn paginated(posts: PostPage) -> serde_json::Value {
    let total_pages = posts.total_pages();
    serde_json::json!({
        "data": posts.posts,
        "pagination": {
            "page": posts.page,
            "per_page": posts.per_page,
            "total": posts.total,
            "total_pages": total_pages,
        }
    })
}

// ── Request bodies ─────────────────────────────────────────────────────

/// Request body for setting the official team response (#205).
#[derive(Debug, Deserialize, ToSchema)]
pub struct OfficialResponseBody {
    /// Comment id to pin as the official response, or null to remove it.
    pub comment_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_body_has_updates() {
        assert!(UpdatePostBody { status: Some("done".into()), category: None }.has_updates());
        assert!(!UpdatePostBody { status: None, category: None }.has_updates());
    }
}
