//! Analytics event capture (#186) — privacy-first, fire-and-forget.
//!
//! Handlers call [`capture`] after a successful mutation/view. The insert
//! runs on a detached tokio task: it can add latency to nothing, break
//! nothing, and never stores IP / cookie / fingerprint — only aggregate
//! counters (`analytics_events` table). This is the data plane for the MCP
//! analysis tools (`get_analytics`, `get_top_posts`) and the admin REST
//! mirror.

use rungu_core::Store;

/// Record an analytics event without blocking or failing the caller.
///
/// `event_type` is one of: `board_view`, `post_view`, `vote`, `comment`,
/// `post_created` (validated at the DB level only by convention — free-form
/// text keeps the schema simple; callers are in this crate).
pub fn capture(store: &Store, project_id: &str, post_id: Option<&str>, event_type: &str) {
    let store = store.clone();
    let project_id = project_id.to_string();
    let post_id = post_id.map(str::to_string);
    let event_type = event_type.to_string();
    tokio::spawn(async move {
        if let Err(e) = store.record_event(&project_id, post_id.as_deref(), &event_type).await {
            // Never propagate — analytics must not break user requests.
            tracing::debug!(error = %e, "analytics capture failed (ignored)");
        }
    });
}
