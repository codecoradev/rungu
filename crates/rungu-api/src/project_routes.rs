//! Project routes — CRUD for projects.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{CreateProjectBody, UpdateProjectBody};

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", axum::routing::get(list_projects).post(create_project))
        .route("/projects/:slug", axum::routing::get(get_project).patch(update_project).delete(delete_project))
}

// ── Handlers ───────────────────────────────────────────────────────────

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
    if user.role != rungu_proto::UserRole::Admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("Project name is required"));
    }
    if name.len() > 100 {
        return Err(ApiError::bad_request("Project name must be 100 characters or less"));
    }

    let slug = match body.slug.as_deref() {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                return Err(ApiError::bad_request("Slug cannot be empty if provided"));
            }
            s.to_string()
        }
        None => slugify(name),
    };

    if !is_valid_slug(&slug) {
        return Err(ApiError::bad_request("Slug must contain only lowercase letters, numbers, and hyphens"));
    }

    let description = body.description.unwrap_or_default();

    let project = state.store.create_project(name, &slug, &description).await?;

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
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

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
    if user.role != rungu_proto::UserRole::Admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    if !body.has_updates() {
        return Err(ApiError::bad_request("No fields to update"));
    }

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let updated = state.store.update_project(&project.id, body.name.as_deref(), body.description.as_deref()).await?;

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
    if user.role != rungu_proto::UserRole::Admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    state.store.delete_project(&project.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Helpers ────────────────────────────────────────────────────────────

fn slugify(name: &str) -> String {
    let slug: String =
        name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() && c.is_ascii() { c } else { '-' }).collect();

    let mut result = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    if result.ends_with('-') {
        result.pop();
    }

    result
}

fn is_valid_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 80 {
        return false;
    }

    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return false;
    }

    !slug.starts_with('-') && !slug.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("My App! @#$"), "my-app");
        assert_eq!(slugify("C++ Project"), "c-project");
    }

    #[test]
    fn test_slugify_multiple_spaces() {
        assert_eq!(slugify("My   Multiple   Spaces"), "my-multiple-spaces");
    }

    #[test]
    fn test_slugify_leading_trailing() {
        assert_eq!(slugify("  Hello  "), "hello");
        assert_eq!(slugify("---test---"), "test");
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("   "), "");
    }

    #[test]
    fn test_slugify_unicode() {
        assert_eq!(slugify("Café Résumé"), "caf-r-sum");
    }

    #[test]
    fn test_is_valid_slug() {
        assert!(is_valid_slug("my-app"));
        assert!(is_valid_slug("myapp"));
        assert!(is_valid_slug("my-app-123"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("-myapp"));
        assert!(!is_valid_slug("myapp-"));
        assert!(!is_valid_slug("MyApp"));
        assert!(!is_valid_slug("my app"));
    }

    #[test]
    fn test_update_body_has_updates() {
        assert!(UpdateProjectBody { name: Some("New".into()), description: None }.has_updates());
        assert!(UpdateProjectBody { name: None, description: Some("Desc".into()) }.has_updates());
        assert!(!UpdateProjectBody { name: None, description: None }.has_updates());
    }
}
