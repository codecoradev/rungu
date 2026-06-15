//! Project routes — CRUD for projects.
//!
//! Follows the same module pattern as `post_routes.rs`:
//! - `pub fn router() -> Router<AppState>`
//! - Uses `crate::error::ApiError` for all errors
//! - Success responses use `Json(serde_json::json!({ "data": ... }))`
//!
//! ```text
//! GET    /api/projects         — list all projects (public)
//! POST   /api/projects         — create project (admin only)
//! GET    /api/projects/:slug   — get single project (public)
//! PATCH  /api/projects/:slug   — update project (admin only)
//! DELETE /api/projects/:slug   — delete project (admin only)
//! ```

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;

// ── Request types ──────────────────────────────────────────────────────

/// Body for `POST /api/projects`.
#[derive(Debug, Deserialize)]
pub struct CreateProjectBody {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
}

/// Body for `PATCH /api/projects/:slug`.
#[derive(Debug, Deserialize)]
pub struct UpdateProjectBody {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl UpdateProjectBody {
    /// Check if at least one field is provided.
    fn has_updates(&self) -> bool {
        self.name.is_some() || self.description.is_some()
    }
}

// ── Routes ─────────────────────────────────────────────────────────────

/// Build project routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", axum::routing::get(list_projects).post(create_project))
        .route("/projects/:slug", axum::routing::get(get_project).patch(update_project).delete(delete_project))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List all projects.
async fn list_projects(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let projects = state.store.list_projects().await?;
    Ok(Json(serde_json::json!({ "data": projects })))
}

/// Create a new project (admin only).
async fn create_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreateProjectBody>,
) -> Result<(StatusCode, impl IntoResponse), ApiError> {
    // Admin check
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

    // Auto-generate slug from name if not provided
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

    // Validate slug format
    if !is_valid_slug(&slug) {
        return Err(ApiError::bad_request("Slug must contain only lowercase letters, numbers, and hyphens"));
    }

    let description = body.description.unwrap_or_default();

    let project = state.store.create_project(name, &slug, &description).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": project }))))
}

/// Get a single project by slug.
async fn get_project(State(state): State<AppState>, Path(slug): Path<String>) -> Result<impl IntoResponse, ApiError> {
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    Ok(Json(serde_json::json!({ "data": project })))
}

/// Update a project (admin only).
async fn update_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdateProjectBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Admin check
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
async fn delete_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    // Admin check
    if user.role != rungu_proto::UserRole::Admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    state.store.delete_project(&project.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Generate a URL-safe slug from a project name.
///
/// Rules: lowercase, spaces → hyphens, remove non-alphanumeric (except hyphens),
/// collapse consecutive hyphens, trim leading/trailing hyphens.
fn slugify(name: &str) -> String {
    let slug: String =
        name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() && c.is_ascii() { c } else { '-' }).collect();

    // Collapse consecutive hyphens and trim
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

    // Trim trailing hyphen
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Validate slug format: lowercase alphanumeric with hyphens, must start/end with alphanumeric.
fn is_valid_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 80 {
        return false;
    }

    // Must be lowercase alphanumeric with hyphens
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return false;
    }

    // Must start and end with alphanumeric
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
