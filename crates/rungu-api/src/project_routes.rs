//! Project routes — CRUD for projects.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_core::Actor;
use rungu_core::ops::{NewProject, ProjectChanges};
use rungu_proto::{CreateProjectBody, UpdateProjectBody};

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", axum::routing::get(list_projects).post(create_project))
        .route("/projects/{slug}", axum::routing::get(get_project).patch(update_project).delete(delete_project))
        .route("/projects/{slug}/counts", axum::routing::get(get_project_counts))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// Public per-project counts grouped by status and category (#202).
/// Powers the board sidebar / mobile filter counts. No PII — aggregates only.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/counts",
    responses(
        (status = 200, description = "Post counts by status and category", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "projects",
)]
pub async fn get_project_counts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let project = state.ops.queries().project(&slug).await?;
    let stats = state.store.project_stats(&project.id).await?;
    Ok(Json(serde_json::json!({
        "data": {
            "total": stats.total_posts,
            "by_status": stats.by_status,
            "by_category": stats.by_category,
        }
    })))
}

/// List all projects.
#[utoipa::path(
    get,
    path = "/api/projects",
    responses(
        (status = 200, description = "List of projects", body = serde_json::Value),
    ),
    tag = "projects",
)]
pub async fn list_projects(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let projects = state.store.list_projects().await?;
    Ok(Json(serde_json::json!({ "data": projects })))
}

/// Create a new project (admin only).
#[utoipa::path(
    post,
    path = "/api/projects",
    request_body = CreateProjectBody,
    responses(
        (status = 201, description = "Project created", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "projects",
)]
pub async fn create_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreateProjectBody>,
) -> Result<(StatusCode, impl IntoResponse), ApiError> {
    let input = NewProject { name: &body.name, slug: body.slug.as_deref(), description: body.description.as_deref() };
    let project = state.ops.create_project(&Actor::from(&user), input).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": project }))))
}

/// Get a single project by slug.
#[utoipa::path(
    get,
    path = "/api/projects/{slug}",
    params(
        ("slug" = String, Path, description = "Project slug"),
    ),
    responses(
        (status = 200, description = "Project detail", body = rungu_proto::Project),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    tag = "projects",
)]
pub async fn get_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let project = state.ops.queries().project(&slug).await?;

    Ok(Json(serde_json::json!({ "data": project })))
}

/// Update a project (admin only).
#[utoipa::path(
    patch,
    path = "/api/projects/{slug}",
    params(
        ("slug" = String, Path, description = "Project slug"),
    ),
    request_body = UpdateProjectBody,
    responses(
        (status = 200, description = "Project updated", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "projects",
)]
pub async fn update_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdateProjectBody>,
) -> Result<impl IntoResponse, ApiError> {
    let changes = ProjectChanges { name: body.name.as_deref(), description: body.description.as_deref() };
    let updated = state.ops.update_project(&Actor::from(&user), &slug, changes).await?;

    Ok(Json(serde_json::json!({ "data": updated })))
}

/// Delete a project (admin only).
#[utoipa::path(
    delete,
    path = "/api/projects/{slug}",
    params(
        ("slug" = String, Path, description = "Project slug"),
    ),
    responses(
        (status = 204, description = "Project deleted"),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "projects",
)]
pub async fn delete_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    state.ops.delete_project(&Actor::from(&user), &slug).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_body_has_updates() {
        assert!(UpdateProjectBody { name: Some("New".into()), description: None }.has_updates());
        assert!(UpdateProjectBody { name: None, description: Some("Desc".into()) }.has_updates());
        assert!(!UpdateProjectBody { name: None, description: None }.has_updates());
    }
}
