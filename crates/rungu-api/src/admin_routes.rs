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
        .route("/admin/analytics/{slug}", axum::routing::get(project_analytics))
        .route("/admin/analytics/{slug}/top", axum::routing::get(project_analytics_top))
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

    let page = query.page.unwrap_or(1).clamp(1, 10_000_000);
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

/// Analytics summary for a project (#186) — event totals + daily trend.
/// Privacy-first: aggregate counts only (no IP / user data is ever stored).
#[utoipa::path(
    get,
    path = "/api/admin/analytics/{slug}",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("days" = Option<u32>, Query, description = "Window in days (default 30, 0 = all time)"),
    ),
    responses(
        (status = 200, description = "Event totals and daily trend", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "admin",
)]
pub async fn project_analytics(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    // Authz BEFORE any query validation per repo convention (#162).
    ApiError::require_admin(&user)?;

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let days = query.days.unwrap_or(30).clamp(0, 365);
    let totals = state.store.analytics_totals(&project.id, days).await?;
    let daily = state.store.analytics_daily(&project.id, days).await?;

    let daily_rows: Vec<serde_json::Value> = daily
        .into_iter()
        .map(|(day, event_type, count)| serde_json::json!({ "day": day, "event_type": event_type, "count": count }))
        .collect();

    Ok(Json(serde_json::json!({
        "data": {
            "project_id": project.id,
            "days": days,
            "totals": totals,
            "daily": daily_rows,
        }
    })))
}

/// Top posts by views for a project (#186) — with vote-count and
/// vote/view conversion joined in for prioritization analysis.
#[utoipa::path(
    get,
    path = "/api/admin/analytics/{slug}/top",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("days" = Option<u32>, Query, description = "Window in days (default 30, 0 = all time)"),
        ("limit" = Option<i64>, Query, description = "Max posts (default 10, max 50)"),
    ),
    responses(
        (status = 200, description = "Top posts by views", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "admin",
)]
pub async fn project_analytics_top(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let days = query.days.unwrap_or(30).clamp(0, 365);
    let limit = query.limit.unwrap_or(10).clamp(1, 50);
    let top = state.store.analytics_top_posts(&project.id, days, limit).await?;

    // Join post title + vote_count for analysis-ready output.
    let mut rows: Vec<serde_json::Value> = Vec::with_capacity(top.len());
    for (post_id, views) in top {
        let title_vote = match state.store.get_post(&post_id, None).await? {
            Some(detail) => (detail.post.title.clone(), detail.post.vote_count),
            None => (String::from("(deleted)"), 0),
        };
        let conversion = if views > 0 { (title_vote.1 as f64 / views as f64 * 1000.0).round() / 10.0 } else { 0.0 };
        rows.push(serde_json::json!({
            "post_id": post_id,
            "title": title_vote.0,
            "views": views,
            "vote_count": title_vote.1,
            "vote_view_pct": conversion,
        }));
    }

    Ok(Json(serde_json::json!({ "data": rows })))
}

/// Query params for the admin analytics endpoints.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct AnalyticsQuery {
    /// Look-back window in days. `0` = all time.
    pub days: Option<u32>,
    /// Max rows for the top-posts endpoint.
    pub limit: Option<i64>,
}
