//! Admin routes — cross-board moderation queue and project statistics.
//!
//! All endpoints require the admin role.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::AppState;
use crate::error::ApiError;

// ── Query params ───────────────────────────────────────────────────────

/// Query params for `GET /api/admin/posts`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdminPostsQuery {
    /// Filter by exact status (`open|planned|in_progress|done|declined`).
    pub status: Option<String>,
    /// Filter by exact project slug.
    pub project: Option<String>,
    /// Page number (1-based).
    pub page: Option<i64>,
    /// Items per page (1-100).
    pub per_page: Option<i64>,
}

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/posts", axum::routing::get(list_all_posts))
        .route("/admin/projects/{slug}/stats", axum::routing::get(project_stats))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List posts across ALL boards — admin moderation queue.
#[utoipa::path(
    get,
    path = "/api/admin/posts",
    params(AdminPostsQuery),
    responses(
        (status = 200, description = "All posts across boards with pagination", body = serde_json::Value),
        (status = 400, description = "Invalid filter value", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "admin",
)]
pub async fn list_all_posts(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<AdminPostsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let status = match query.status.as_deref() {
        Some(s) => {
            Some(crate::post_routes::parse_status(s).ok_or_else(|| ApiError::bad_request("Invalid status filter"))?)
        }
        None => None,
    };

    let (posts, total) = state
        .store
        .list_all_posts(rungu_core::ListAllPostsParams {
            status,
            project_slug: query.project.as_deref(),
            offset,
            limit: per_page,
        })
        .await?;

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

/// Aggregate counts for a project — used by the safe-delete dialog to preview
/// cascade consequences ("N posts, N votes, N comments will be deleted").
#[utoipa::path(
    get,
    path = "/api/admin/projects/{slug}/stats",
    params(("slug" = String, Path, description = "Project slug")),
    responses(
        (status = 200, description = "Project aggregate counts", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "admin",
)]
pub async fn project_stats(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let stats = state.store.project_stats(&project.id).await?;

    Ok(Json(serde_json::json!({ "data": stats })))
}
