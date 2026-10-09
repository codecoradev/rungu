//! Analytics event capture (#186) — privacy-first, fire-and-forget.
//!
//! Handlers call [`capture`] after a successful mutation/view. The insert
//! runs on a detached tokio task: it can add latency to nothing, break
//! nothing, and never stores IP / cookie / fingerprint — only aggregate
//! counters (`analytics_events` table). This is the data plane for the MCP
//! analysis tools (`get_analytics`, `get_top_posts`) and the admin REST
//! mirror.

use rungu_core::{DomainEvent, EventSink, Store};

/// Records mutation counters from domain events. View events (`board_view`,
/// `post_view`) are reads, so their handlers still call [`capture`] directly.
pub struct AnalyticsSink {
    store: Store,
}

impl AnalyticsSink {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl EventSink for AnalyticsSink {
    fn publish(&self, event: &DomainEvent) {
        let (post_id, event_type) = match event {
            DomainEvent::PostCreated { post, .. } => (&post.id, "post_created"),
            DomainEvent::CommentCreated { comment, .. } => (&comment.post_id, "comment"),
            // Cast or retracted — still a high-intent signal.
            DomainEvent::VoteToggled { post_id, .. } => (post_id, "vote"),
            _ => return,
        };
        capture(&self.store, event.project_id(), Some(post_id), event_type);
    }
}

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
    crate::background::spawn(async move {
        if let Err(e) = store.record_event(&project_id, post_id.as_deref(), &event_type).await {
            // Never propagate — analytics must not break user requests.
            tracing::debug!(error = %e, "analytics capture failed (ignored)");
        }
    });
}
