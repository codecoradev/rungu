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

use rungu_proto::{
    Attachment, CommentDetail, CurrentUser, Post, PostCategory, PostDetail, PostSort, PostStatus, Project, UserRole,
    Webhook,
};

use crate::events::{DomainEvent, EventBus};
use crate::validation::{MAX_PROJECT_NAME_LEN, MAX_WEBHOOK_URL_LEN, is_valid_slug, slugify, validate_webhook_url};
use crate::{MAX_UPLOAD_SIZE, Storage, Store, storage_key, verify_image};

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
    /// The request clashes with existing state (e.g. a slug already taken).
    #[error("{0}")]
    Conflict(String),
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
    fn conflict(msg: String) -> Self {
        Self::Conflict(msg)
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

    fn require_admin(&self) -> OpResult<()> {
        if self.is_admin() { Ok(()) } else { Err(OpError::forbidden("Admin access required")) }
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

/// Input for [`Operations::create_project`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NewProject<'a> {
    pub name: &'a str,
    /// Absent = derived from the name. Present but blank is rejected.
    pub slug: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Input for [`Operations::update_project`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectChanges<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Input for [`Operations::create_webhook`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NewWebhook<'a> {
    pub url: &'a str,
    /// Absent = all events (`*`). Present but blank is rejected.
    pub events: Option<&'a str>,
    /// Absent or empty = a random secret is generated; otherwise used as is.
    pub secret: Option<&'a str>,
}

/// Input for [`Operations::update_webhook`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WebhookChanges<'a> {
    pub url: Option<&'a str>,
    pub events: Option<&'a str>,
    pub is_active: Option<bool>,
}

/// Result of [`Operations::create_webhook`]: the stored webhook plus its
/// signing secret, which is only ever returned here.
#[derive(Debug, Clone)]
pub struct CreatedWebhook {
    pub webhook: Webhook,
    pub secret: String,
}

/// Input for [`Operations::upload_attachment`]: an already-read upload.
#[derive(Debug, Clone)]
pub struct NewAttachment<'a> {
    pub filename: &'a str,
    pub content_type: &'a str,
    pub data: Vec<u8>,
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

        // Re-saving the current value is a no-op: no write (so `updated_at`
        // and the changelog order don't move) and no event (so nobody is
        // emailed "moved to Open" for a post that was already open).
        if let Some(new_status) = status.filter(|s| *s != before.status) {
            self.store.update_post_status(post_id, new_status).await?;
            self.events.publish(DomainEvent::PostStatusChanged {
                post: before.clone(),
                old_status: before.status,
                new_status,
                actor_id: actor.id.clone(),
            });
        }
        if let Some(new_category) = category.filter(|c| *c != before.category) {
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

    // ── Projects (admin) ─────────────────────────────────────────────

    pub async fn create_project(&self, actor: &Actor, input: NewProject<'_>) -> OpResult<Project> {
        actor.require_admin()?;

        let name = validate_project_name(input.name)?;
        let slug = match input.slug {
            Some(s) => {
                let s = s.trim();
                if s.is_empty() {
                    return Err(OpError::invalid("Slug cannot be empty if provided"));
                }
                s.to_string()
            }
            None => slugify(name),
        };
        if !is_valid_slug(&slug) {
            return Err(OpError::invalid("Slug must contain only lowercase letters, numbers, and hyphens"));
        }
        let taken = || OpError::conflict(format!("A project with slug '{slug}' already exists"));
        // Uncached: the slug cache may still hold a project deleted elsewhere.
        if self.store.project_slug_exists(&slug).await? {
            return Err(taken());
        }
        // A concurrent create can still win between the check and the insert;
        // its UNIQUE violation is the same conflict, not a server error.
        match self.store.create_project(name, &slug, input.description.unwrap_or("")).await {
            Ok(project) => Ok(project),
            Err(e) if is_unique_violation(&e) => Err(taken()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn update_project(&self, actor: &Actor, slug: &str, changes: ProjectChanges<'_>) -> OpResult<Project> {
        actor.require_admin()?;
        // Admin-only, so checking input before existence leaks nothing.
        if changes.name.is_none() && changes.description.is_none() {
            return Err(OpError::invalid("No fields to update"));
        }
        if changes.name.is_some_and(|n| n.trim().is_empty()) {
            return Err(OpError::invalid("Project name cannot be empty"));
        }
        let name = changes.name.map(validate_project_name).transpose()?;
        let project = self.existing_project(slug).await?;

        Ok(self.store.update_project(&project.id, name, changes.description).await?)
    }

    /// Delete a project and everything in it, including the attachment
    /// files of its posts (the FK cascade only removes the rows).
    pub async fn delete_project(&self, actor: &Actor, slug: &str) -> OpResult<Project> {
        actor.require_admin()?;
        let project = self.existing_project(slug).await?;

        let blob_paths = self.store.list_project_attachment_paths(&project.id).await?;
        self.store.delete_project(&project.id).await?;
        for path in blob_paths {
            self.delete_blob(&path).await;
        }
        Ok(project)
    }

    // ── Webhooks (admin) ─────────────────────────────────────────────

    pub async fn create_webhook(&self, actor: &Actor, slug: &str, input: NewWebhook<'_>) -> OpResult<CreatedWebhook> {
        actor.require_admin()?;
        let project = self.existing_project(slug).await?;

        let url = validate_new_webhook_url(input.url)?;
        let events = match input.events {
            None => "*",
            Some(e) if e.trim().is_empty() => return Err(OpError::invalid("Webhook events cannot be empty")),
            Some(e) => e,
        };
        // A supplied secret is used exactly as given (receivers sign with it);
        // absent or empty means generate one.
        let secret = match input.secret {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => uuid::Uuid::new_v4().to_string(),
        };

        let webhook = self.store.create_webhook(&project.id, url, events, &secret).await?;
        Ok(CreatedWebhook { webhook, secret })
    }

    pub async fn update_webhook(
        &self,
        actor: &Actor,
        slug: &str,
        webhook_id: &str,
        changes: WebhookChanges<'_>,
    ) -> OpResult<Webhook> {
        actor.require_admin()?;
        let project = self.existing_project(slug).await?;
        self.project_webhook(&project, webhook_id).await?;

        let url = changes.url.map(validate_new_webhook_url).transpose()?;
        self.store
            .update_webhook(webhook_id, url, changes.events, changes.is_active)
            .await?
            .ok_or_else(|| OpError::not_found("Webhook not found"))
    }

    /// Delete a webhook. With `slug`, the webhook must also belong to that
    /// project (REST always passes it; the MCP tool addresses webhooks by id).
    pub async fn delete_webhook(&self, actor: &Actor, webhook_id: &str, slug: Option<&str>) -> OpResult<Webhook> {
        actor.require_admin()?;
        let webhook = match slug {
            Some(slug) => {
                let project = self.existing_project(slug).await?;
                self.project_webhook(&project, webhook_id).await?
            }
            None => self.store.get_webhook(webhook_id).await?.ok_or_else(|| OpError::not_found("Webhook not found"))?,
        };
        self.store.delete_webhook(webhook_id).await?;
        Ok(webhook)
    }

    async fn existing_project(&self, slug: &str) -> OpResult<Project> {
        self.store.get_project_by_slug(slug).await?.ok_or_else(|| OpError::not_found("Project not found"))
    }

    /// A webhook of another project reads as not found, so ids can't be
    /// probed across projects.
    async fn project_webhook(&self, project: &Project, webhook_id: &str) -> OpResult<Webhook> {
        self.store
            .get_webhook(webhook_id)
            .await?
            .filter(|w| w.project_id == project.id)
            .ok_or_else(|| OpError::not_found("Webhook not found"))
    }

    // ── Attachments ──────────────────────────────────────────────────

    /// Whether `actor` may attach files to `post_id` (post author or admin).
    /// Transports call this before reading an upload body, so a refused
    /// upload is never buffered; [`Self::upload_attachment`] checks it again.
    pub async fn check_can_upload(&self, actor: &Actor, post_id: &str) -> OpResult<()> {
        let post = self.existing_post(post_id).await?;
        actor.require_owner_or_admin(&post.created_by, "Only the post author or admin can upload attachments")
    }

    /// Store an uploaded image on a post: verify it, save the file, then the
    /// row. If the row can't be written, the file is removed again.
    pub async fn upload_attachment(
        &self,
        actor: &Actor,
        post_id: &str,
        upload: NewAttachment<'_>,
    ) -> OpResult<Attachment> {
        self.check_can_upload(actor, post_id).await?;

        if upload.data.is_empty() {
            return Err(OpError::invalid("Empty file"));
        }
        if upload.data.len() > MAX_UPLOAD_SIZE {
            return Err(OpError::invalid("File too large"));
        }
        let mime = verify_image(&upload.data, upload.content_type).map_err(|e| OpError::Invalid(e.to_string()))?;

        let size = upload.data.len() as i64;
        let key = storage_key(&uuid::Uuid::new_v4().to_string(), &mime);
        self.storage.save(&key, upload.data).await?;

        match self.store.create_attachment(post_id, upload.filename, &mime, size, &key, &actor.id).await {
            Ok(attachment) => Ok(attachment),
            Err(e) => {
                tracing::warn!(error = %e, key = %key, "DB insert failed; removing the orphaned attachment file");
                self.delete_blob(&key).await;
                Err(OpError::Internal(e))
            }
        }
    }

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

fn validate_project_name(name: &str) -> OpResult<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::invalid("Project name is required"));
    }
    if name.len() > MAX_PROJECT_NAME_LEN {
        return Err(OpError::invalid("Project name must be 100 characters or less"));
    }
    Ok(name)
}

/// SQLite and Postgres both report a UNIQUE violation in the error text.
fn is_unique_violation(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        let msg = cause.to_string();
        msg.contains("UNIQUE constraint failed") || msg.contains("duplicate key value")
    })
}

fn validate_new_webhook_url(url: &str) -> OpResult<&str> {
    let url = url.trim();
    if url.is_empty() {
        return Err(OpError::invalid("Webhook URL is required"));
    }
    if url.len() > MAX_WEBHOOK_URL_LEN {
        return Err(OpError::invalid("URL must be 2048 characters or less"));
    }
    validate_webhook_url(url).map_err(OpError::Invalid)?;
    Ok(url)
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
