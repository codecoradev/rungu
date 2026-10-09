//! Domain operations — the single home of every mutating use case.
//!
//! REST handlers and MCP tools are thin adapters over [`Operations`]: they
//! decode input, call one method, and encode the result. Each method owns
//! the whole use case — existence check, authorization, validation, the
//! store write, storage cleanup, and the [`DomainEvent`] that drives side
//! effects — so both transports get identical behaviour.
//!
//! Check order is deliberate: existence → authorization → input validation.
//! Validating before the ownership check would let non-owners probe which
//! posts exist (#162).

use std::sync::Arc;

use rungu_proto::{CommentDetail, CurrentUser, Post, PostCategory, PostDetail, PostSort, PostStatus, UserRole};

use crate::events::{DomainEvent, EventBus};
use crate::{Storage, Store};

/// Maximum post title length, in bytes after trimming.
pub const MAX_TITLE_LEN: usize = 200;
/// Maximum comment length, in bytes after trimming.
pub const MAX_COMMENT_LEN: usize = 4000;

/// Why an operation was refused. Adapters map each variant once
/// (HTTP status code, JSON-RPC error).
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Forbidden(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl OpError {
    fn not_found(msg: &str) -> Self {
        Self::NotFound(msg.to_string())
    }
    fn invalid(msg: &str) -> Self {
        Self::Invalid(msg.to_string())
    }
    fn forbidden(msg: &str) -> Self {
        Self::Forbidden(msg.to_string())
    }
}

pub type OpResult<T> = Result<T, OpError>;

/// Who is performing an operation.
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: String,
    pub role: UserRole,
}

impl Actor {
    pub fn is_admin(&self) -> bool {
        self.role == UserRole::Admin
    }

    fn require_owner_or_admin(&self, owner_id: &str, msg: &str) -> OpResult<()> {
        if self.id == owner_id || self.is_admin() { Ok(()) } else { Err(OpError::forbidden(msg)) }
    }
}

impl From<&CurrentUser> for Actor {
    fn from(user: &CurrentUser) -> Self {
        Self { id: user.id.clone(), role: user.role }
    }
}

/// Input for [`Operations::create_post`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NewPost<'a> {
    pub title: &'a str,
    pub description: &'a str,
    /// Absent or empty = `feedback`; anything else must be a known category.
    pub category: Option<&'a str>,
}

/// Input for [`Operations::update_post`]. Raw strings so every adapter
/// shares one parser and one set of error messages.
#[derive(Debug, Clone, Copy, Default)]
pub struct PostChanges<'a> {
    pub status: Option<&'a str>,
    pub category: Option<&'a str>,
}

/// Result of [`Operations::toggle_vote`].
#[derive(Debug, Clone, Copy)]
pub struct VoteOutcome {
    pub voted: bool,
    pub vote_count: i64,
}

/// Result of [`Operations::set_official_response`].
#[derive(Debug, Clone)]
pub struct OfficialResponseOutcome {
    pub previous_comment_id: Option<String>,
    pub response: Option<CommentDetail>,
}

/// The domain operations module. Cheap to clone; clones share the store
/// (and its project cache), the storage backend, and the event bus.
#[derive(Clone)]
pub struct Operations {
    store: Store,
    storage: Arc<dyn Storage>,
    events: EventBus,
}

impl Operations {
    pub fn new(store: Store, storage: Arc<dyn Storage>, events: EventBus) -> Self {
        Self { store, storage, events }
    }

    /// Read access for query paths, which stay on the store directly.
    pub fn store(&self) -> &Store {
        &self.store
    }

    async fn existing_post(&self, post_id: &str) -> OpResult<Post> {
        Ok(self.store.get_post(post_id, None).await?.ok_or_else(|| OpError::not_found("Post not found"))?.post)
    }

    // ── Posts ────────────────────────────────────────────────────────

    pub async fn create_post(&self, actor: &Actor, project_slug: &str, input: NewPost<'_>) -> OpResult<Post> {
        let project = self
            .store
            .get_project_by_slug(project_slug)
            .await?
            .ok_or_else(|| OpError::not_found("Project not found"))?;

        let title = input.title.trim();
        if title.is_empty() {
            return Err(OpError::invalid("Title is required"));
        }
        if title.len() > MAX_TITLE_LEN {
            return Err(OpError::invalid("Title must be 200 characters or less"));
        }
        let category = match input.category {
            None | Some("") => PostCategory::Feedback,
            Some(s) => parse_category(s).ok_or_else(|| OpError::invalid("Invalid category"))?,
        };

        let post = self.store.create_post(&project.id, title, input.description, category, &actor.id).await?;
        self.events.publish(DomainEvent::PostCreated { post: post.clone(), actor_id: actor.id.clone() });
        Ok(post)
    }

    /// Change status and/or category (author or admin). Both fields are
    /// validated before anything is written, so a bad category never leaves
    /// a half-applied status change behind.
    pub async fn update_post(&self, actor: &Actor, post_id: &str, changes: PostChanges<'_>) -> OpResult<PostDetail> {
        let before = self.existing_post(post_id).await?;
        actor.require_owner_or_admin(&before.created_by, "You can only update your own posts")?;

        if changes.status.is_none() && changes.category.is_none() {
            return Err(OpError::invalid("No fields to update"));
        }
        let status =
            changes.status.map(|s| parse_status(s).ok_or_else(|| OpError::invalid("Invalid status"))).transpose()?;
        let category = changes
            .category
            .map(|c| parse_category(c).ok_or_else(|| OpError::invalid("Invalid category")))
            .transpose()?;

        if let Some(new_status) = status {
            self.store.update_post_status(post_id, new_status).await?;
            self.events.publish(DomainEvent::PostStatusChanged {
                post: before.clone(),
                old_status: before.status,
                new_status,
                actor_id: actor.id.clone(),
            });
        }
        if let Some(new_category) = category {
            self.store.update_post_category(post_id, new_category).await?;
            self.events.publish(DomainEvent::PostCategoryChanged {
                post: before.clone(),
                old_category: before.category,
                new_category,
                actor_id: actor.id.clone(),
            });
        }

        self.store
            .get_post(post_id, Some(&actor.id))
            .await?
            .ok_or_else(|| OpError::Internal(anyhow::anyhow!("Post disappeared after update")))
    }

    /// Delete a post (author or admin), including its attachment files —
    /// the FK cascade only removes the attachment rows.
    pub async fn delete_post(&self, actor: &Actor, post_id: &str) -> OpResult<()> {
        let post = self.existing_post(post_id).await?;
        actor.require_owner_or_admin(&post.created_by, "You can only delete your own posts")?;

        let mut blob_paths = Vec::new();
        for attachment in self.store.list_attachments(post_id).await? {
            if let Some((_, path)) = self.store.get_attachment(&attachment.id).await? {
                blob_paths.push(path);
            }
        }

        self.store.delete_post(post_id).await?;
        for path in blob_paths {
            self.delete_blob(&path).await;
        }

        self.events.publish(DomainEvent::PostDeleted { post, actor_id: actor.id.clone() });
        Ok(())
    }

    // ── Votes ────────────────────────────────────────────────────────

    pub async fn toggle_vote(&self, actor: &Actor, post_id: &str) -> OpResult<VoteOutcome> {
        let post = self.existing_post(post_id).await?;
        let voted = self.store.toggle_vote(&actor.id, post_id).await?;
        let vote_count = self.store.get_post(post_id, None).await?.map(|p| p.post.vote_count).unwrap_or(0);

        self.events.publish(DomainEvent::VoteToggled {
            post_id: post.id,
            project_id: post.project_id,
            voted,
            vote_count,
            actor_id: actor.id.clone(),
        });
        Ok(VoteOutcome { voted, vote_count })
    }

    // ── Comments ─────────────────────────────────────────────────────

    pub async fn create_comment(
        &self,
        actor: &Actor,
        post_id: &str,
        content: &str,
        parent_id: Option<&str>,
    ) -> OpResult<CommentDetail> {
        let content = content.trim();
        if content.is_empty() {
            return Err(OpError::invalid("Comment content is required"));
        }
        if content.len() > MAX_COMMENT_LEN {
            return Err(OpError::invalid("Comment must be 4000 characters or less"));
        }

        let post = self.existing_post(post_id).await?;

        if let Some(parent_id) = parent_id {
            let parent =
                self.store.get_comment(parent_id).await?.ok_or_else(|| OpError::invalid("Parent comment not found"))?;
            if parent.post_id != post_id {
                return Err(OpError::invalid("Parent comment does not belong to this post"));
            }
        }

        let comment = self.store.create_comment(post_id, content, parent_id, &actor.id).await?;
        self.events.publish(DomainEvent::CommentCreated {
            comment: comment.comment.clone(),
            project_id: post.project_id,
            post_title: post.title,
            actor_id: actor.id.clone(),
        });
        Ok(comment)
    }

    pub async fn delete_comment(&self, actor: &Actor, comment_id: &str) -> OpResult<()> {
        let comment =
            self.store.get_comment(comment_id).await?.ok_or_else(|| OpError::not_found("Comment not found"))?;
        actor.require_owner_or_admin(&comment.created_by, "You can only delete your own comments")?;

        let project_id = self.existing_post(&comment.post_id).await?.project_id;
        self.store.delete_comment(comment_id).await?;

        self.events.publish(DomainEvent::CommentDeleted { comment, project_id, actor_id: actor.id.clone() });
        Ok(())
    }

    /// Pin (`Some`) or unpin (`None`) a comment as the post's official team
    /// response. Admin only.
    pub async fn set_official_response(
        &self,
        actor: &Actor,
        post_id: &str,
        comment_id: Option<&str>,
    ) -> OpResult<OfficialResponseOutcome> {
        if !actor.is_admin() {
            return Err(OpError::forbidden("Admin access required"));
        }
        let post = self.existing_post(post_id).await?;

        if let Some(cid) = comment_id {
            if cid.trim().is_empty() {
                return Err(OpError::invalid("comment_id cannot be empty"));
            }
            let belongs = self.store.get_comment(cid).await?.is_some_and(|c| c.post_id == post.id);
            if !belongs {
                return Err(OpError::invalid("Comment does not belong to this post"));
            }
        }

        let previous_comment_id = self.store.set_official_response(&post.id, comment_id).await?;
        let response = self.store.get_official_response(&post.id).await?;

        self.events.publish(DomainEvent::OfficialResponseChanged {
            post_id: post.id,
            project_id: post.project_id,
            comment_id: comment_id.map(str::to_string),
            previous_comment_id: previous_comment_id.clone(),
            actor_id: actor.id.clone(),
        });
        Ok(OfficialResponseOutcome { previous_comment_id, response })
    }

    // ── Attachments ──────────────────────────────────────────────────

    /// Delete an attachment (uploader or admin): the file first, then the row.
    pub async fn delete_attachment(&self, actor: &Actor, attachment_id: &str) -> OpResult<()> {
        let (attachment, storage_path) = self
            .store
            .get_attachment(attachment_id)
            .await?
            .ok_or_else(|| OpError::not_found("Attachment not found"))?;
        actor.require_owner_or_admin(&attachment.created_by, "Only the uploader or admin can delete attachments")?;

        self.delete_blob(&storage_path).await;
        self.store.delete_attachment(attachment_id).await?;
        Ok(())
    }

    /// A missing file must not block deleting its record — log and move on.
    async fn delete_blob(&self, path: &str) {
        if let Err(e) = self.storage.delete(path).await {
            tracing::warn!("Failed to delete attachment file {path}: {e}");
        }
    }
}

// ── Shared parsers ───────────────────────────────────────────────────

pub fn parse_status(s: &str) -> Option<PostStatus> {
    match s {
        "open" => Some(PostStatus::Open),
        "planned" => Some(PostStatus::Planned),
        "in_progress" => Some(PostStatus::InProgress),
        "done" => Some(PostStatus::Done),
        "declined" => Some(PostStatus::Declined),
        _ => None,
    }
}

pub fn parse_category(s: &str) -> Option<PostCategory> {
    match s {
        "feedback" => Some(PostCategory::Feedback),
        "bug" => Some(PostCategory::Bug),
        "feature" => Some(PostCategory::Feature),
        "question" => Some(PostCategory::Question),
        _ => None,
    }
}

/// Unknown or absent sort keys fall back to newest-first.
pub fn parse_sort(s: Option<&str>) -> PostSort {
    match s {
        Some("oldest") => PostSort::Oldest,
        Some("most_votes") => PostSort::MostVotes,
        Some("least_votes") => PostSort::LeastVotes,
        Some("recently_updated") => PostSort::RecentlyUpdated,
        Some("trending") => PostSort::Trending,
        _ => PostSort::Newest,
    }
}
