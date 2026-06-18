//! Attachment routes — image upload, list, serve, delete.

use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use rungu_proto::{AttachmentResponse, UserRole};
use utoipa::ToSchema;

use crate::AppState;
use crate::error::ApiError;

// ── Routes ─────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/posts/{id}/attachments", axum::routing::get(list_attachments).post(upload_attachment))
        .route("/attachments/{id}", axum::routing::get(get_attachment_file).delete(delete_attachment))
}

// ── Responses ─────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct AttachmentListResponse {
    pub data: Vec<AttachmentResponse>,
}

// ── Handlers ──────────────────────────────────────────────────────────

/// Upload an image attachment to a post.
///
/// Multipart form with field name `file`. Max 10 MB.
/// Allowed types: PNG, JPEG, WebP, GIF.
#[utoipa::path(
    post,
    path = "/api/posts/{id}/attachments",
    params(("id" = String, Path, description = "Post ID")),
    responses(
        (status = 201, description = "Attachment uploaded", body = AttachmentResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Not authorized"),
        (status = 413, description = "File too large"),
        (status = 415, description = "Unsupported media type"),
    ),
    tag = "attachments",
)]
pub async fn upload_attachment(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    CurrentUser(user): CurrentUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    // Verify post exists
    let post = state
        .store
        .get_post(&post_id, None)
        .await
        .map_err(|_| ApiError::internal_default())?
        .ok_or_else(|| ApiError::not_found("Post not found"))?;

    // Only post author or admin can upload
    if post.post.created_by != user.id && user.role != UserRole::Admin {
        return Err(ApiError::forbidden("Only the post author or admin can upload attachments"));
    }

    // Process multipart — find the "file" field
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => return Err(ApiError::bad_request("Invalid multipart data")),
        };
        if field.name() != Some("file") {
            continue;
        }

        let filename = field.file_name().unwrap_or("upload").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();

        // Read field data with manual size limit enforcement
        let mut data = Vec::new();
        let mut total = 0usize;
        let mut field = field;
        while let Some(chunk) = field.chunk().await.map_err(|_| ApiError::bad_request("Failed to read upload"))? {
            total += chunk.len();
            if total > rungu_core::MAX_UPLOAD_SIZE {
                return Err(ApiError::payload_too_large_default());
            }
            data.extend_from_slice(&chunk);
        }
        let data: Vec<u8> = data;

        if data.is_empty() {
            return Err(ApiError::bad_request("Empty file"));
        }

        // Verify image — magic byte check
        let verified_mime =
            rungu_core::verify_image(&data, &content_type).map_err(|e| ApiError::bad_request(e.to_string()))?;

        // Generate storage key
        let attachment_id = uuid::Uuid::new_v4().to_string();
        let key = rungu_core::storage_key(&attachment_id, &verified_mime);

        // Save to storage
        state.storage.save(&key, data.to_vec()).await.map_err(|_| ApiError::internal_default())?;

        // Save metadata to DB
        let attachment = state
            .store
            .create_attachment(&post_id, &filename, &verified_mime, data.len() as i64, &key, &user.id)
            .await
            .map_err(|_| ApiError::internal_default())?;

        let attachment_id = attachment.id.clone();
        let response = AttachmentResponse {
            id: attachment.id,
            post_id: attachment.post_id,
            filename: attachment.filename,
            mime: attachment.mime,
            size: attachment.size,
            url: format!("/api/attachments/{}", attachment_id),
            created_by: attachment.created_by,
            created_at: attachment.created_at,
        };

        return Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": response }))));
    }

    Err(ApiError::bad_request("No file field in multipart data"))
}

/// List all attachments for a post.
#[utoipa::path(
    get,
    path = "/api/posts/{id}/attachments",
    params(("id" = String, Path, description = "Post ID")),
    responses(
        (status = 200, description = "List of attachments", body = AttachmentListResponse),
    ),
    tag = "attachments",
)]
pub async fn list_attachments(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify post exists
    let _ = state
        .store
        .get_post(&post_id, None)
        .await
        .map_err(|_| ApiError::internal_default())?
        .ok_or_else(|| ApiError::not_found("Post not found"))?;

    let attachments = state.store.list_attachments(&post_id).await.map_err(|_| ApiError::internal_default())?;

    let data: Vec<AttachmentResponse> = attachments
        .into_iter()
        .map(|a| AttachmentResponse {
            url: format!("/api/attachments/{}", a.id),
            id: a.id,
            post_id: a.post_id,
            filename: a.filename,
            mime: a.mime,
            size: a.size,
            created_by: a.created_by,
            created_at: a.created_at,
        })
        .collect();

    Ok(Json(AttachmentListResponse { data }))
}

/// Serve an attachment file (streams the image).
#[utoipa::path(
    get,
    path = "/api/attachments/{id}",
    params(("id" = String, Path, description = "Attachment ID")),
    responses(
        (status = 200, description = "Image file"),
        (status = 404, description = "Not found"),
    ),
    tag = "attachments",
)]
pub async fn get_attachment_file(
    State(state): State<AppState>,
    Path(attachment_id): Path<String>,
) -> Result<Response, ApiError> {
    let (attachment, storage_path) = state
        .store
        .get_attachment(&attachment_id)
        .await
        .map_err(|_| ApiError::internal_default())?
        .ok_or_else(|| ApiError::not_found("Attachment not found"))?;

    let data = state.storage.load(&storage_path).await.map_err(|_| ApiError::not_found("File not found in storage"))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, attachment.mime),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"{}\"", attachment.filename)),
            (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ("x-content-type-options".parse::<axum::http::HeaderName>().unwrap(), "nosniff".to_string()),
        ],
        data,
    )
        .into_response())
}

/// Delete an attachment (author or admin only).
#[utoipa::path(
    delete,
    path = "/api/attachments/{id}",
    params(("id" = String, Path, description = "Attachment ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Not authorized"),
        (status = 404, description = "Not found"),
    ),
    tag = "attachments",
)]
pub async fn delete_attachment(
    State(state): State<AppState>,
    Path(attachment_id): Path<String>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, ApiError> {
    let (attachment, storage_path) = state
        .store
        .get_attachment(&attachment_id)
        .await
        .map_err(|_| ApiError::internal_default())?
        .ok_or_else(|| ApiError::not_found("Attachment not found"))?;

    // Only attachment creator or admin can delete
    if attachment.created_by != user.id && user.role != UserRole::Admin {
        return Err(ApiError::forbidden("Only the uploader or admin can delete attachments"));
    }

    // Delete file from storage
    if let Err(e) = state.storage.delete(&storage_path).await {
        tracing::warn!("Failed to delete attachment file {storage_path}: {e}");
    }

    // Delete DB record
    state.store.delete_attachment(&attachment_id).await.map_err(|_| ApiError::internal_default())?;

    Ok(StatusCode::NO_CONTENT)
}
