//! MCP over HTTP (#189) — remote AI access to the same 26 tools as stdio.
//!
//! Speaks both MCP eras (see `rungu_mcp`): the stateless `2026-07-28`
//! revision, whose `Mcp-Method` / `Mcp-Name` / `MCP-Protocol-Version` headers
//! are checked against the body (HTTP 400 on mismatch), and the legacy
//! `initialize` handshake. Notifications are answered `202`; `GET`/`DELETE`
//! are `405` (no server-initiated streams, no sessions).
//!
//! `POST /mcp` with `Authorization: Bearer <RUNGU_API_KEY>` carries a JSON-RPC
//! request; the response is the JSON-RPC result. This is the stateless subset
//! of the MCP Streamable HTTP transport that mainstream clients configure as
//! `{"url": "…", "headers": {"Authorization": "Bearer …"}}`. The stdio
//! transport remains the local default — this route simply reuses
//! `rungu_mcp::handle_message` over a second transport, with zero duplicated
//! tool logic.
//!
//! The transport decides who is acting: a valid API key is the same
//! `ai-agent` identity the REST API gives that key, so rows an agent creates
//! over `/mcp` and over REST carry the same author. With an OAuth issuer
//! configured (see [`crate::mcp_oauth`]) an access token from that issuer is
//! accepted too and acts as the person it names, with their normal Rungu role.
//!
//! This module also serves the OAuth discovery documents (RFC 9728 / 8414)
//! and answers browser preflights: `/mcp` is called from claude.ai, so it owns
//! its CORS and `Origin` check instead of the site-wide layer.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use rungu_mcp::handle_message;

use crate::mcp_oauth::Reject;

/// The actor behind a valid API key: the bootstrapped `ai-agent` user with the
/// admin role (the same identity the REST Bearer path uses).
fn api_key_actor(state: &crate::AppState) -> rungu_core::Actor {
    rungu_core::Actor {
        id: state.agent_user_id.as_deref().unwrap_or(rungu_auth::middleware::AGENT_USER_EMAIL).to_string(),
        role: rungu_proto::UserRole::Admin,
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Add the CORS headers for an allowed browser origin (echoed, never `*`: the
/// response depends on a credential).
fn with_cors(mut response: Response, origin: Option<&str>) -> Response {
    let headers = response.headers_mut();
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    if let Some(origin) = origin.and_then(|o| HeaderValue::from_str(o).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("www-authenticate, mcp-session-id"),
        );
    }
    response
}

fn unauthorized(state: &crate::AppState, message: &str) -> Response {
    let mut response = (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": message }))).into_response();
    let challenge = match &state.mcp.oauth {
        Some(oauth) => oauth.challenge(),
        None => "Bearer".to_string(),
    };
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

/// Who is calling: the API key, or an access token from the configured issuer.
async fn authenticate(
    state: &crate::AppState,
    bearer: Option<&str>,
) -> Result<(rungu_core::Actor, &'static str), (Response, &'static str)> {
    let Some(presented) = bearer else {
        return Err((
            unauthorized(state, "Missing Authorization: Bearer <access token or RUNGU_API_KEY>"),
            "missing_token",
        ));
    };
    if rungu_auth::middleware::verify_api_key(&state.config, presented) {
        return Ok((api_key_actor(state), "api_key"));
    }
    if let Some(oauth) = &state.mcp.oauth {
        return match oauth.authenticate(presented, &state.store).await {
            Ok(actor) => Ok((actor, "oauth")),
            Err(reason) => {
                let message = match reason {
                    Reject::IssuerUnavailable => "Could not verify the access token right now",
                    _ => "Invalid access token",
                };
                Err((unauthorized(state, message), reason.as_str()))
            }
        };
    }
    Err((unauthorized(state, "Invalid API key"), "invalid_api_key"))
}

/// POST /mcp — one JSON-RPC request per HTTP call (stateless). Runs on the
/// server's shared `Operations`, so it shares the project cache and event
/// subscribers with the REST API.
pub async fn mcp_http(State(state): State<crate::AppState>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let origin = header_str(&headers, "origin");
    if let Some(origin) = origin
        && !state.mcp.origin_allowed(origin, &state.config.app_url)
    {
        tracing::warn!(origin, "Rejected /mcp request from a disallowed Origin");
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Origin not allowed" }))).into_response();
    }
    let allowed_origin = origin;

    let response = handle_post(&state, &headers, &body).await;
    with_cors(response, allowed_origin)
}

async fn handle_post(state: &crate::AppState, headers: &HeaderMap, body: &[u8]) -> Response {
    // Auth: the API key (constant-time compare) or an OAuth access token.
    let bearer = header_str(headers, "authorization")
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (actor, auth) = match authenticate(state, bearer).await {
        Ok(ok) => ok,
        Err((response, reason)) => {
            // Debugging a remote client needs the reason; never the token or body.
            tracing::warn!(
                reason,
                ua = header_str(headers, "user-agent").unwrap_or(""),
                pv = header_str(headers, "mcp-protocol-version").unwrap_or(""),
                mm = header_str(headers, "mcp-method").unwrap_or(""),
                accept = header_str(headers, "accept").unwrap_or(""),
                "Rejected /mcp request"
            );
            return response;
        }
    };

    // Size gate BEFORE any parse work — reject oversized bodies first.
    if body.len() > 1_048_576 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32600, "message": "Request too large (max 1MB)" }, "id": null })),
        )
            .into_response();
    }

    let Ok(input) = std::str::from_utf8(body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32700, "message": "Body must be UTF-8 JSON" }, "id": null })),
        )
            .into_response();
    };

    // Header checks need the body's method, so parse once here; a body that
    // isn't JSON falls through to the dispatcher's own parse error.
    let parsed = serde_json::from_str::<serde_json::Value>(input).ok();
    tracing::info!(
        method = parsed.as_ref().and_then(|m| m.get("method")).and_then(|m| m.as_str()).unwrap_or(""),
        auth,
        ua = header_str(headers, "user-agent").unwrap_or(""),
        pv = header_str(headers, "mcp-protocol-version").unwrap_or(""),
        mm = header_str(headers, "mcp-method").unwrap_or(""),
        accept = header_str(headers, "accept").unwrap_or(""),
        "MCP request"
    );
    if let Some(message) = &parsed {
        if message.is_array() {
            let response = handle_message(input, &state.ops, &actor).await;
            return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "application/json")], response).into_response();
        }
        if let Err(e) = rungu_mcp::validate_headers(
            header_str(headers, "mcp-protocol-version"),
            header_str(headers, "mcp-method"),
            header_str(headers, "mcp-name"),
            message,
        ) {
            tracing::warn!("Rejected /mcp request headers: {e:?}");
            return (StatusCode::BAD_REQUEST, Json(e.to_json(message.get("id").cloned()))).into_response();
        }
    }

    let response = handle_message(input, &state.ops, &actor).await;
    // A notification has no reply: acknowledge it with 202 and no body.
    if response.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], response).into_response()
}

/// OPTIONS /mcp — the browser preflight (claude.ai calls `/mcp` from a page).
async fn mcp_preflight(State(state): State<crate::AppState>, headers: HeaderMap) -> Response {
    let Some(origin) = header_str(&headers, "origin") else {
        return StatusCode::NO_CONTENT.into_response();
    };
    if !state.mcp.origin_allowed(origin, &state.config.app_url) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    let h = response.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("POST, OPTIONS"));
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, accept, mcp-protocol-version, mcp-method, mcp-name, mcp-session-id, last-event-id",
        ),
    );
    h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
    with_cors(response, Some(origin))
}

/// Discovery documents are public and carry no credentials, so any origin may read them.
fn public_json(value: serde_json::Value) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    response
}

fn oauth_off() -> Response {
    let mut response = StatusCode::NOT_FOUND.into_response();
    response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    response
}

/// GET /.well-known/oauth-protected-resource[/mcp] — RFC 9728.
async fn protected_resource_metadata(State(state): State<crate::AppState>) -> Response {
    match &state.mcp.oauth {
        Some(oauth) => public_json(oauth.protected_resource_metadata()),
        None => oauth_off(),
    }
}

/// GET /.well-known/oauth-authorization-server[/mcp] and
/// /.well-known/openid-configuration — the issuer's own metadata, unchanged,
/// for clients that look for it on the resource origin (an unknown path would
/// otherwise fall into the SPA and answer 200 with HTML).
async fn issuer_metadata(State(state): State<crate::AppState>) -> Response {
    let Some(oauth) = &state.mcp.oauth else {
        return oauth_off();
    };
    match oauth.issuer_metadata().await {
        Ok(doc) => public_json(doc),
        Err(_) => {
            let mut response = StatusCode::BAD_GATEWAY.into_response();
            response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            response
        }
    }
}

/// Anything but POST/OPTIONS on `/mcp`: no server-initiated streams, no sessions.
async fn mcp_method_not_allowed(method: Method) -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response.headers_mut().insert(header::ALLOW, HeaderValue::from_static("POST, OPTIONS"));
    tracing::debug!(%method, "Rejected /mcp method");
    response
}

/// MCP-over-HTTP routes and OAuth discovery documents — mounted at root.
pub fn mcp_routes() -> axum::Router<crate::AppState> {
    axum::Router::new()
        .route("/mcp", post(mcp_http).options(mcp_preflight).fallback(mcp_method_not_allowed))
        .route("/.well-known/oauth-protected-resource", get(protected_resource_metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(protected_resource_metadata))
        .route("/.well-known/oauth-authorization-server", get(issuer_metadata))
        .route("/.well-known/oauth-authorization-server/mcp", get(issuer_metadata))
        .route("/.well-known/openid-configuration", get(issuer_metadata))
}
