//! Notification preferences and unsubscribe (#73).
//!
//! Unsubscribe is GET-to-confirm, POST-to-act: mail security scanners
//! (e.g. Outlook Safe Links) prefetch every link in a message, so a GET
//! that changed state would unsubscribe people who never clicked. Mail
//! clients use the POST directly via the RFC 8058 `List-Unsubscribe-Post`
//! header; people click the confirmation page's button.

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
        .route("/notifications/unsubscribe", get(unsubscribe_page).post(unsubscribe))
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesBody {
    notifications_opt_out: bool,
}

/// Get the caller's email-notification preference.
#[utoipa::path(get, path = "/api/me/notifications/preferences", responses(
    (status = 200, description = "Current preference: `{ notificationsOptOut }`", body = serde_json::Value),
    (status = 401, description = "Not authenticated"),
), security(("session" = [])), tag = "notifications")]
pub async fn get_preferences(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let opt_out = state.store.notifications_opt_out(&user.id).await?;
    Ok(Json(serde_json::json!({ "notificationsOptOut": opt_out })))
}

/// Update the caller's email-notification preference.
#[utoipa::path(post, path = "/api/me/notifications/preferences", request_body = PreferencesBody, responses(
    (status = 200, description = "Preference updated", body = serde_json::Value),
    (status = 400, description = "Invalid body"),
    (status = 401, description = "Not authenticated"),
), security(("session" = [])), tag = "notifications")]
pub async fn set_preferences(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<PreferencesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.store.set_notifications_opt_out(&user.id, body.notifications_opt_out).await? {
        return Err(ApiError::not_found("User not found"));
    }
    Ok(Json(serde_json::json!({ "notificationsOptOut": body.notifications_opt_out })))
}

#[derive(Deserialize)]
pub struct UnsubscribeQuery {
    token: String,
}

fn verified_user(state: &AppState, token: &str) -> Result<String, ApiError> {
    email::verify_unsubscribe_token(token, &state.config.app_secret)
        .ok_or_else(|| ApiError::bad_request("Invalid unsubscribe link"))
}

/// Confirmation page for the unsubscribe link. Changes nothing.
#[utoipa::path(get, path = "/api/notifications/unsubscribe", params(
    ("token" = String, Query, description = "Signed unsubscribe token from the email"),
), responses(
    (status = 200, description = "Confirmation page (HTML)"),
    (status = 400, description = "Invalid token"),
), tag = "notifications")]
pub async fn unsubscribe_page(
    State(state): State<AppState>,
    Query(q): Query<UnsubscribeQuery>,
) -> Result<Html<String>, ApiError> {
    verified_user(&state, &q.token)?;
    // Token is verified (hex + uuid), but escape anyway: it lands in HTML.
    let action = format!("?token={}", urlencoding::encode(&q.token));
    Ok(page(
        "Unsubscribe from email notifications?",
        &format!(
            "<p>You'll stop getting emails about new comments and status changes.</p>\
             <form method=\"post\" action=\"{}\"><button type=\"submit\">Unsubscribe</button></form>",
            action.replace('&', "&amp;").replace('"', "&quot;")
        ),
    ))
}

/// Unsubscribe — the confirmation button and RFC 8058 one-click POST.
#[utoipa::path(post, path = "/api/notifications/unsubscribe", params(
    ("token" = String, Query, description = "Signed unsubscribe token from the email"),
), responses(
    (status = 200, description = "Unsubscribed (idempotent)"),
    (status = 400, description = "Invalid token"),
), tag = "notifications")]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(q): Query<UnsubscribeQuery>,
) -> Result<Html<String>, ApiError> {
    let user_id = verified_user(&state, &q.token)?;
    // A deleted user has nothing to unsubscribe; same answer either way.
    state.store.set_notifications_opt_out(&user_id, true).await?;
    Ok(page("You're unsubscribed", "<p>You won't get email notifications from this board anymore.</p>"))
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;\
         color:#111;padding:1rem}}.card{{max-width:28rem;text-align:center}}h1{{font-size:1.25rem}}p{{color:#444}}\
         button{{font:inherit;padding:.6rem 1.2rem;min-height:44px;border-radius:.5rem;border:0;background:#1d4ed8;\
         color:#fff;cursor:pointer}}</style></head><body><main class=\"card\"><h1>{title}</h1>{body}</main></body></html>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_body_is_camel_case() {
        let b: PreferencesBody = serde_json::from_str(r#"{"notificationsOptOut":true}"#).unwrap();
        assert!(b.notifications_opt_out);
    }
}
