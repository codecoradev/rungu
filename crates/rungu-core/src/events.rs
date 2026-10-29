//! Domain events and the fan-out seam for mutation side effects.
//!
//! Every successful mutation in [`crate::ops::Operations`] publishes exactly
//! one [`DomainEvent`] to an [`EventBus`]. Side effects (webhooks, analytics,
//! email notifications) are [`EventSink`] adapters subscribed to the bus, so
//! adding a new side effect never touches the REST or MCP adapters.

use std::sync::{Arc, Mutex};

use rungu_proto::{Comment, Post, PostCategory, PostStatus};

/// Something that happened to the board, published after the write commits.
///
/// `actor_id` is the user who caused the change; subscribers use it to avoid
/// notifying people about their own actions.
#[derive(Debug, Clone)]
pub enum DomainEvent {
    PostCreated {
        post: Post,
        actor_id: String,
    },
    /// `post` is the snapshot taken before the change (title, project, author).
    PostStatusChanged {
        post: Post,
        old_status: PostStatus,
        new_status: PostStatus,
        actor_id: String,
    },
    PostCategoryChanged {
        post: Post,
        old_category: PostCategory,
        new_category: PostCategory,
        actor_id: String,
    },
    /// `post` is the snapshot taken before deletion.
    PostDeleted {
        post: Post,
        actor_id: String,
    },
    CommentCreated {
        comment: Comment,
        project_id: String,
        post_title: String,
        actor_id: String,
    },
    CommentDeleted {
        comment: Comment,
        project_id: String,
        actor_id: String,
    },
    VoteToggled {
        post_id: String,
        project_id: String,
        voted: bool,
        vote_count: i64,
        actor_id: String,
    },
    OfficialResponseChanged {
        post_id: String,
        project_id: String,
        comment_id: Option<String>,
        previous_comment_id: Option<String>,
        actor_id: String,
    },
}

impl DomainEvent {
    /// Project the event belongs to (webhook subscriptions are per project).
    pub fn project_id(&self) -> &str {
        match self {
            Self::PostCreated { post, .. }
            | Self::PostStatusChanged { post, .. }
            | Self::PostCategoryChanged { post, .. }
            | Self::PostDeleted { post, .. } => &post.project_id,
            Self::CommentCreated { project_id, .. }
            | Self::CommentDeleted { project_id, .. }
            | Self::VoteToggled { project_id, .. }
            | Self::OfficialResponseChanged { project_id, .. } => project_id,
        }
    }

    pub fn actor_id(&self) -> &str {
        match self {
            Self::PostCreated { actor_id, .. }
            | Self::PostStatusChanged { actor_id, .. }
            | Self::PostCategoryChanged { actor_id, .. }
            | Self::PostDeleted { actor_id, .. }
            | Self::CommentCreated { actor_id, .. }
            | Self::CommentDeleted { actor_id, .. }
            | Self::VoteToggled { actor_id, .. }
            | Self::OfficialResponseChanged { actor_id, .. } => actor_id,
        }
    }
}

/// A subscriber to domain events.
///
/// `publish` runs on the request path, so implementations must not block or
/// fail the caller: spawn any I/O onto a detached task and log errors there.
pub trait EventSink: Send + Sync {
    fn publish(&self, event: &DomainEvent);
}

/// Fans each event out to every subscribed sink. Cheap to clone.
#[derive(Clone, Default)]
pub struct EventBus {
    sinks: Arc<Vec<Arc<dyn EventSink>>>,
}

impl EventBus {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks: Arc::new(sinks) }
    }

    pub fn publish(&self, event: DomainEvent) {
        for sink in self.sinks.iter() {
            sink.publish(&event);
        }
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus").field("sinks", &self.sinks.len()).finish()
    }
}

/// In-memory sink that keeps every event — the test adapter for the seam.
#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<DomainEvent>>,
}

impl RecordingSink {
    pub fn events(&self) -> Vec<DomainEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl EventSink for RecordingSink {
    fn publish(&self, event: &DomainEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }
    }
}
