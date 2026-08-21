//! Notification preference + one-click unsubscribe routes (issue #73).

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use rungu_auth::CurrentUser;
use serde::Deserialize;

use crate::AppState;
use crate::email;
use crate::error::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/notifications/preferences", get(get_preferences).post(set_preferences))
        .route("/notifications/unsubscribe", get(unsubscribe))
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesBody {
    notifications_opt_out: bool,
}

/// Get the caller's email-notification preference.
#[utoipa::path(get, path = "/api/notifications/preferences", responses(
    (status = 200, description = "Current preference", body = serde_json::Value),
    (status = 401, description = "Not authenticated"),
), security(("session" = [])), tag = "notifications")]
pub async fn get_preferences(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let u = state.store.get_user(&user.id).await?.ok_or_else(|| ApiError::internal("User not found"))?;
    Ok(Json(serde_json::json!({ "notificationsOptOut": u.notifications_opt_out })))
}

/// Update the caller's email-notification preference.
#[utoipa::path(post, path = "/api/notifications/preferences", responses(
    (status = 200, description = "Preference updated", body = serde_json::Value),
    (status = 400, description = "Invalid body"),
    (status = 401, description = "Not authenticated"),
), security(("session" = [])), tag = "notifications")]
pub async fn set_preferences(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<PreferencesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.set_notifications_opt_out(&user.id, body.notifications_opt_out).await?;
    Ok(Json(serde_json::json!({ "notificationsOptOut": body.notifications_opt_out })))
}

#[derive(Deserialize)]
pub struct UnsubscribeQuery {
    token: String,
}

/// One-click unsubscribe from email links. GET so it works from any mail
/// client without JS. Renders a tiny confirmation page.
#[utoipa::path(get, path = "/api/notifications/unsubscribe", params(
    ("token" = String, Query, description = "HMAC unsubscribe token"),
), responses(
    (status = 200, description = "Unsubscribed (or already unsubscribed)"),
    (status = 400, description = "Invalid token"),
), tag = "notifications")]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(q): Query<UnsubscribeQuery>,
) -> Result<Html<&'static str>, ApiError> {
    let user_id = email::verify_unsubscribe_token(&q.token, &state.config.app_secret)
        .ok_or_else(|| ApiError::bad_request("Invalid unsubscribe token"))?;

    state.store.set_notifications_opt_out(&user_id, true).await?;
    Ok(Html(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Unsubscribed</title>\
         <style>body{font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;color:#111}\
         .card{text-align:center}h1{font-size:1.25rem}p{color:#555}</style></head>\
         <body><div class=\"card\"><h1>You're unsubscribed</h1>\
         <p>You will no longer receive email notifications. You can re-enable them anytime from your profile settings.</p></div></body></html>",
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_body_camelcase() {
        let b: PreferencesBody = serde_json::from_str(r#"{"notificationsOptOut":true}"#).unwrap();
        assert!(b.notifications_opt_out);
    }
}
