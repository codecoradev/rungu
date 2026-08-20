//! Webhook routes — CRUD management + delivery log.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{CreateWebhookBody, UpdateWebhookBody};
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{slug}/webhooks", axum::routing::get(list_webhooks).post(create_webhook))
        .route(
            "/projects/{slug}/webhooks/{id}",
            axum::routing::get(get_webhook).patch(update_webhook).delete(delete_webhook),
        )
        .route("/projects/{slug}/webhooks/{id}/deliveries", axum::routing::get(list_deliveries))
        .route("/projects/{slug}/webhooks/{id}/test", axum::routing::post(test_webhook))
}

// ── Handlers (test) ────────────────────────────────────────────────────

/// Send a test event to a webhook (admin only). Synchronous single delivery
/// so the admin UI can show the actual result inline.
#[utoipa::path(
    post,
    path = "/api/projects/{slug}/webhooks/{id}/test",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("id" = String, Path, description = "Webhook ID"),
    ),
    responses(
        (status = 200, description = "Test delivery result", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Webhook not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn test_webhook(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let webhook = state.store.get_webhook(&id).await?.ok_or_else(|| ApiError::not_found("Webhook not found"))?;
    if webhook.project_id != project.id {
        return Err(ApiError::not_found("Webhook not found"));
    }
    if !webhook.is_active {
        return Err(ApiError::bad_request("Webhook is not active"));
    }

    let secret =
        state.store.get_webhook_secret(&id).await?.ok_or_else(|| ApiError::not_found("Webhook secret not found"))?;

    let payload = serde_json::json!({
        "event": "webhook.test",
        "project": { "id": project.id, "slug": project.slug, "name": project.name },
        "webhook_id": webhook.id,
        "sent_at": chrono::Utc::now().to_rfc3339(),
    });
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let signature = crate::webhook::sign_payload(&payload_str, &secret);

    let result = crate::webhook::deliver_once(&state.http_client, &webhook, &payload_str, &signature).await;

    // Truncate on a char boundary (payload may contain multi-byte UTF-8).
    let payload_excerpt: String = payload_str.chars().take(2000).collect();
    match result {
        Err((attempts, msg)) => {
            let _ = state
                .store
                .record_webhook_delivery(
                    &webhook.id,
                    "webhook.test",
                    &payload_excerpt,
                    None,
                    false,
                    attempts as i32,
                    &msg,
                )
                .await;
            Ok(Json(serde_json::json!({ "ok": false, "attempts": attempts, "error": msg })))
        }
        Ok(status_code) => {
            let ok = (200..300).contains(&status_code);
            let msg = if ok { "" } else { &format!("HTTP {status_code}") };
            let _ = state
                .store
                .record_webhook_delivery(&webhook.id, "webhook.test", &payload_excerpt, Some(status_code), ok, 1, msg)
                .await;
            Ok(Json(serde_json::json!({ "ok": ok, "status": status_code })))
        }
    }
}

// ── Query params ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeliveryQuery {
    pub limit: Option<i64>,
}

// ── Handlers ───────────────────────────────────────────────────────────

/// List webhooks for a project (admin only).
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/webhooks",
    params(("slug" = String, Path, description = "Project slug")),
    responses(
        (status = 200, description = "List of webhooks", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn list_webhooks(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;
    let webhooks = state.store.list_webhooks(&project.id).await?;
    Ok(Json(serde_json::json!({ "data": webhooks })))
}

/// Create a webhook subscription (admin only).
#[utoipa::path(
    post,
    path = "/api/projects/{slug}/webhooks",
    params(("slug" = String, Path, description = "Project slug")),
    request_body = CreateWebhookBody,
    responses(
        (status = 201, description = "Webhook created", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Project not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<CreateWebhookBody>,
) -> Result<(StatusCode, impl IntoResponse), ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let url = body.url.trim();
    if url.is_empty() {
        return Err(ApiError::bad_request("Webhook URL is required"));
    }
    if url.len() > 2048 {
        return Err(ApiError::bad_request("URL must be 2048 characters or less"));
    }

    crate::webhook::validate_webhook_url(url).map_err(ApiError::bad_request)?;

    let events = body.events.unwrap_or_else(|| "*".to_string());
    // Generate a random secret if the user didn't provide one.
    let secret = body.secret.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let webhook = state.store.create_webhook(&project.id, url, &events, &secret).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": webhook }))))
}

/// Get webhook details (admin only).
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/webhooks/{id}",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("id" = String, Path, description = "Webhook ID"),
    ),
    responses(
        (status = 200, description = "Webhook detail", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Webhook not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn get_webhook(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    CurrentUser(user): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;
    let webhook = state.store.get_webhook(&id).await?.ok_or_else(|| ApiError::not_found("Webhook not found"))?;

    if webhook.project_id != project.id {
        return Err(ApiError::not_found("Webhook not found"));
    }

    Ok(Json(serde_json::json!({ "data": webhook })))
}

/// Update a webhook (admin only).
#[utoipa::path(
    patch,
    path = "/api/projects/{slug}/webhooks/{id}",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("id" = String, Path, description = "Webhook ID"),
    ),
    request_body = UpdateWebhookBody,
    responses(
        (status = 200, description = "Webhook updated", body = serde_json::Value),
        (status = 400, description = "Validation error", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Webhook not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn update_webhook(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<UpdateWebhookBody>,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    if let Some(ref url) = body.url {
        let url = url.trim();
        if url.is_empty() {
            return Err(ApiError::bad_request("Webhook URL cannot be empty"));
        }
        crate::webhook::validate_webhook_url(url).map_err(ApiError::bad_request)?;
    }

    // Verify ownership before updating.
    let existing = state.store.get_webhook(&id).await?.ok_or_else(|| ApiError::not_found("Webhook not found"))?;
    if existing.project_id != project.id {
        return Err(ApiError::not_found("Webhook not found"));
    }

    let url_ref = body.url.as_deref().map(str::trim);
    let updated = state
        .store
        .update_webhook(&id, url_ref, body.events.as_deref(), body.is_active)
        .await?
        .ok_or_else(|| ApiError::not_found("Webhook not found"))?;

    Ok(Json(serde_json::json!({ "data": updated })))
}

/// Delete a webhook (admin only).
#[utoipa::path(
    delete,
    path = "/api/projects/{slug}/webhooks/{id}",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("id" = String, Path, description = "Webhook ID"),
    ),
    responses(
        (status = 204, description = "Webhook deleted"),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Webhook not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let webhook = state.store.get_webhook(&id).await?.ok_or_else(|| ApiError::not_found("Webhook not found"))?;
    if webhook.project_id != project.id {
        return Err(ApiError::not_found("Webhook not found"));
    }

    state.store.delete_webhook(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List delivery log for a webhook (admin only).
#[utoipa::path(
    get,
    path = "/api/projects/{slug}/webhooks/{id}/deliveries",
    params(
        ("slug" = String, Path, description = "Project slug"),
        ("id" = String, Path, description = "Webhook ID"),
        ("limit" = Option<i64>, Query, description = "Max results (default 50)"),
    ),
    responses(
        (status = 200, description = "Delivery log", body = serde_json::Value),
        (status = 401, description = "Not authenticated", body = serde_json::Value),
        (status = 403, description = "Admin access required", body = serde_json::Value),
        (status = 404, description = "Webhook not found", body = serde_json::Value),
    ),
    security(("session" = [])),
    tag = "webhooks",
)]
pub async fn list_deliveries(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<DeliveryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    ApiError::require_admin(&user)?;
    let project =
        state.store.get_project_by_slug(&slug).await?.ok_or_else(|| ApiError::not_found("Project not found"))?;

    let webhook = state.store.get_webhook(&id).await?.ok_or_else(|| ApiError::not_found("Webhook not found"))?;
    if webhook.project_id != project.id {
        return Err(ApiError::not_found("Webhook not found"));
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let deliveries = state.store.list_webhook_deliveries(&id, limit).await?;
    Ok(Json(serde_json::json!({ "data": deliveries })))
}
