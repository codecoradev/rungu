//! MCP over HTTP (#189) — remote AI access to the same 26 tools as stdio.
//!
//! `POST /mcp` with `Authorization: Bearer <RUNGU_API_KEY>` carries a JSON-RPC
//! request; the response is the JSON-RPC result. This is the stateless subset
//! of the MCP Streamable HTTP transport that mainstream clients configure as
//! `{"url": "…", "headers": {"Authorization": "Bearer …"}}`. The stdio
//! transport remains the local default — this route simply reuses
//! `rungu_mcp::handle_message` over a second transport, with zero duplicated
//! tool logic.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use rungu_mcp::handle_message;

/// POST /mcp — one JSON-RPC request per HTTP call (stateless; each request
/// opens its own pool-backed Store, mirroring the stdio loop).
pub async fn mcp_http(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Auth: same API key as the REST Bearer path (constant-time in the
    // extractor, but this route reads raw headers — compare here too).
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let Some(presented) = bearer else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing Authorization: Bearer <RUNGU_API_KEY>" })),
        )
            .into_response();
    };

    if !rungu_auth::middleware::verify_api_key(&state.config, presented) {
        tracing::warn!("Rejected /mcp request with invalid API key");
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Invalid API key" }))).into_response();
    }

    let Ok(input) = std::str::from_utf8(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32700, "message": "Body must be UTF-8 JSON" }, "id": null })),
        )
            .into_response();
    };

    if input.len() > 1_048_576 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({ "jsonrpc": "2.0", "error": { "code": -32600, "message": "Request too large (max 1MB)" }, "id": null })),
        )
            .into_response();
    }

    let response = handle_message(input, &state.store.pool_for_mcp(), state.store.is_sqlite()).await;
    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/json")], response).into_response()
}

/// MCP-over-HTTP routes — mounted at root (`/mcp`).
pub fn mcp_routes() -> axum::Router<crate::AppState> {
    axum::Router::new().route("/mcp", post(mcp_http))
}
