//! # rungu-mcp
//!
//! MCP server — AI agent tools over JSON-RPC 2.0.
//!
//! [`handle_message`] is transport-independent: it takes one JSON-RPC message
//! and the [`Actor`] the transport authenticated. Two transports use it:
//!
//! - **stdio** ([`run_server`], `rungu mcp`): a local subprocess of a trusted
//!   agent, running as the built-in admin ([`local_admin_actor`]).
//! - **HTTP** (`POST /mcp`, in `rungu-api`): Bearer API key, acting as the
//!   `ai-agent` identity that key has on the REST API.
//!
//! ## ⚠️ Security model
//!
//! Neither transport has per-user authorization: both act as an admin. Whoever
//! can spawn `rungu mcp`, or holds `RUNGU_API_KEY`, can read and write all
//! feedback data. Mutating tools execute through the same [`Operations`] module
//! as the REST API — so validation, storage cleanup, webhooks, and analytics
//! behave identically — and the actor is chosen by the transport, never inside
//! this crate.
//!
//! Never expose `rungu mcp` over a network, never share the database file with
//! untrusted users, keep `RUNGU_API_KEY` secret, and be aware that
//! prompt-injection payloads can instruct the host agent to call these tools.
//! See `docs/integrations/mcp.md#trust-boundary--security` for the full
//! guidance.
//!
//! ## Errors
//!
//! Failed calls return a JSON-RPC error whose code says why ([`McpError`]):
//! `-32602` invalid params, `-32003` forbidden, `-32004` not found, `-32009`
//! conflict, `-32601` unknown method, `-32603` internal.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use rungu_core::ops::{NewPost, NewProject, NewWebhook, PostChanges};
use rungu_core::query::{BoardFilters, PageRequest, PostPage};
use rungu_core::{Actor, BoardQuery, OpError, Operations, Store};
use rungu_proto::*;
use serde_json::{Value, json};

/// Maximum input line length (1 MB) to prevent unbounded memory usage.
const MAX_INPUT_LEN: usize = 1_048_576;

/// Process a single JSON-RPC message and return the response string.
///
/// `actor` is who the transport authenticated: mutating tools act as that
/// actor, so authorization is decided by the transport and enforced by
/// [`Operations`], never hard-coded here.
pub async fn handle_message(input: &str, ops: &Operations, actor: &Actor) -> String {
    if input.len() > MAX_INPUT_LEN {
        return serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "error": {"code": -32600, "message": "Request too large (max 1MB)"},
            "id": null
        }))
        .unwrap();
    }

    let msg: Value = match serde_json::from_str(input.trim()) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": format!("Parse error: {e}")},
                "id": null
            }))
            .unwrap();
        }
    };

    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let params = msg.get("params").cloned().unwrap_or(json!({}));

    // JSON-RPC notifications (no "id") get NO response — replying corrupts
    // protocol-conformant client streams. Side effects still run.
    if id.is_none() {
        let _ = handle_request(&method, &params, ops, actor).await;
        return String::new();
    }

    let result = handle_request(&method, &params, ops, actor).await;

    match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }).to_string(),
        Err(e) => e.to_json(id).to_string(),
    }
}

/// Route a method to its handler with parsed params.
async fn handle_request(method: &str, params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let store = ops.store();
    match method {
        "list_projects" => list_projects(store).await,
        "get_project" => get_project(params, store).await,
        "list_posts" => list_posts(params, store).await,
        "get_post" => get_post(params, store).await,
        "create_post" => create_post(params, ops, actor).await,
        "update_post_status" => update_post_status(params, ops, actor).await,
        "vote_post" => vote_post(params, ops, actor).await,
        "search_posts" => search_posts(params, store).await,
        "get_changelog" => get_changelog(params, store).await,
        "list_comments" => list_comments(params, store).await,
        "add_comment" => add_comment(params, ops, actor).await,
        "delete_comment" => delete_comment(params, ops, actor).await,
        "get_stats" => get_stats(params, store).await,
        "get_trending" => get_trending(params, store).await,
        "get_analytics" => get_analytics(params, store).await,
        "get_top_posts" => get_top_posts(params, store).await,
        "create_project" => create_project(params, ops, actor).await,
        "delete_project" => delete_project(params, ops, actor).await,
        "list_webhooks" => list_webhooks(params, store).await,
        "create_webhook" => create_webhook(params, ops, actor).await,
        "delete_webhook" => delete_webhook(params, ops, actor).await,
        "list_attachments" => list_attachments(params, store).await,
        "delete_post" => delete_post(params, ops, actor).await,
        "update_post_category" => update_post_category(params, ops, actor).await,
        "get_roadmap" => get_roadmap(params, store).await,
        "delete_attachment" => delete_attachment(params, ops, actor).await,
        _ => Err(McpError::method_not_found(format!("Unknown method: {method}"))),
    }
}

// ── Param extraction helpers ───────────────────────────────────────────

fn get_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, McpError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params(format!("Missing required parameter: {key}")))
}

fn get_optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

fn get_optional_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(|v| v.as_u64())
}

/// A post that must exist for the call to make sense. A missing post is
/// `not_found` (as in REST), not an internal error.
async fn existing_post(store: &Store, post_id: &str) -> Result<PostDetail, McpError> {
    store
        .get_post(post_id, None)
        .await
        .map_err(|e| format!("Failed to get post: {e}"))?
        .ok_or_else(|| McpError::not_found(format!("Post not found: {post_id}")))
}

fn queries(store: &Store) -> BoardQuery {
    BoardQuery::new(store.clone())
}

/// `page` (1-based) and `limit` (page size); the query module clamps both.
fn page_request(params: &Value) -> PageRequest {
    let to_i64 = |n: u64| n.min(i64::MAX as u64) as i64;
    PageRequest {
        page: get_optional_u64(params, "page").map(to_i64),
        per_page: get_optional_u64(params, "limit").map(to_i64),
    }
}

/// The MCP shape of a page of posts: `{ data, total, page, per_page, total_pages }`.
fn post_page(posts: PostPage) -> Value {
    let total_pages = posts.total_pages();
    json!({
        "data": posts.posts,
        "total": posts.total,
        "page": posts.page,
        "per_page": posts.per_page,
        "total_pages": total_pages,
    })
}

/// A failed tool call, as a JSON-RPC error.
///
/// Codes follow JSON-RPC where it has one (`-32602` invalid params, `-32601`
/// method not found, `-32603` internal) and use the implementation-defined
/// server-error range for the refusals clients most need to tell apart.
/// They mirror the REST statuses: 403 → `-32003`, 404 → `-32004`,
/// 409 → `-32009`. `data.kind` repeats the reason as a stable string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpError {
    pub code: i64,
    pub kind: &'static str,
    pub message: String,
}

impl McpError {
    pub const INVALID_PARAMS: i64 = -32602;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INTERNAL: i64 = -32603;
    pub const FORBIDDEN: i64 = -32003;
    pub const NOT_FOUND: i64 = -32004;
    pub const CONFLICT: i64 = -32009;

    fn new(code: i64, kind: &'static str, message: impl Into<String>) -> Self {
        Self { code, kind, message: message.into() }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID_PARAMS, "invalid_params", message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(Self::NOT_FOUND, "not_found", message)
    }

    fn method_not_found(message: impl Into<String>) -> Self {
        Self::new(Self::METHOD_NOT_FOUND, "method_not_found", message)
    }

    fn to_json(&self, id: Option<Value>) -> Value {
        json!({
            "jsonrpc": "2.0",
            "error": { "code": self.code, "message": self.message, "data": { "kind": self.kind } },
            "id": id,
        })
    }
}

/// Operation refusals keep their message and gain a code that says why.
impl From<OpError> for McpError {
    fn from(e: OpError) -> Self {
        let message = e.to_string();
        match e {
            OpError::Invalid(_) => Self::invalid_params(message),
            OpError::NotFound(_) => Self::not_found(message),
            OpError::Forbidden(_) => Self::new(Self::FORBIDDEN, "forbidden", message),
            OpError::Conflict(_) => Self::new(Self::CONFLICT, "conflict", message),
            OpError::Internal(_) => Self::new(Self::INTERNAL, "internal", message),
        }
    }
}

/// Handler failures that carry only a message are internal errors.
impl From<String> for McpError {
    fn from(message: String) -> Self {
        Self::new(Self::INTERNAL, "internal", message)
    }
}

fn op_err(e: OpError) -> McpError {
    e.into()
}

/// The identity `rungu mcp` (stdio) runs tools as: the built-in
/// `mcp@rungu.local` user with the admin role. Stdio is a local subprocess
/// of a trusted agent (see the module docs), so it is deliberately an admin.
/// Resolved once at startup; other transports pass the actor they authenticated.
pub async fn local_admin_actor(store: &Store) -> Result<Actor> {
    let user = store
        .find_or_create_user("mcp@rungu.local", Some("MCP Bot"), None, &[])
        .await
        .context("creating the local MCP admin user (is the database writable?)")?;
    Ok(Actor { id: user.id, role: UserRole::Admin })
}

// ── Tool implementations ───────────────────────────────────────────────

/// List all projects.
async fn list_projects(store: &Store) -> Result<Value, McpError> {
    let projects = store.list_projects().await.map_err(|e| format!("Failed to list projects: {e}"))?;
    Ok(json!({ "data": projects }))
}

async fn get_project(params: &Value, store: &Store) -> Result<Value, McpError> {
    let project = queries(store).project(get_str(params, "slug")?).await.map_err(op_err)?;
    Ok(json!({ "data": project }))
}

/// List posts in a project with optional filters (same rules as REST).
async fn list_posts(params: &Value, store: &Store) -> Result<Value, McpError> {
    let filters = BoardFilters {
        sort: get_optional_str(params, "sort"),
        status: get_optional_str(params, "status"),
        category: get_optional_str(params, "category"),
        q: get_optional_str(params, "q"),
    };
    let (_, posts) =
        queries(store).board(get_str(params, "slug")?, filters, page_request(params), None).await.map_err(op_err)?;
    Ok(post_page(posts))
}

/// Get a single post by ID.
async fn get_post(params: &Value, store: &Store) -> Result<Value, McpError> {
    let post = existing_post(store, get_str(params, "id")?).await?;
    Ok(json!({ "data": post }))
}

/// Create a new post.
async fn create_post(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let input = NewPost {
        title: get_str(params, "title")?,
        description: get_optional_str(params, "description").unwrap_or(""),
        category: get_optional_str(params, "category"),
    };
    let post = ops.create_post(actor, get_str(params, "slug")?, input).await.map_err(op_err)?;
    Ok(json!({ "data": post }))
}

/// Update a post's status.
async fn update_post_status(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let id = get_str(params, "id")?;
    let status_str = get_str(params, "status")?;
    ops.update_post(actor, id, PostChanges { status: Some(status_str), category: None }).await.map_err(op_err)?;
    Ok(json!({ "updated": true, "id": id, "status": status_str }))
}

/// Toggle vote on a post.
async fn vote_post(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let id = get_str(params, "id")?;
    let outcome = ops.toggle_vote(actor, id).await.map_err(op_err)?;
    Ok(json!({ "data": { "voted": outcome.voted, "vote_count": outcome.vote_count } }))
}

/// Full-text search, newest first.
async fn search_posts(params: &Value, store: &Store) -> Result<Value, McpError> {
    let posts = queries(store)
        .search(get_str(params, "slug")?, get_str(params, "q")?, page_request(params))
        .await
        .map_err(op_err)?;
    Ok(post_page(posts))
}

/// Shipped posts, most recently updated first. Optional `since` (RFC 3339).
async fn get_changelog(params: &Value, store: &Store) -> Result<Value, McpError> {
    let posts = queries(store)
        .changelog(get_str(params, "slug")?, get_optional_str(params, "since"), page_request(params))
        .await
        .map_err(op_err)?;
    Ok(post_page(posts))
}

/// List comments for a post.
async fn list_comments(params: &Value, store: &Store) -> Result<Value, McpError> {
    let post_id = get_str(params, "post_id")?;
    // Same as REST: comments of a post that doesn't exist are a not-found,
    // not an empty list.
    existing_post(store, post_id).await?;
    let comments = store.list_comments(post_id).await.map_err(|e| format!("Failed to list comments: {e}"))?;

    Ok(json!({ "data": comments }))
}

/// Add a comment to a post.
async fn add_comment(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let post_id = get_str(params, "post_id")?;
    let content = get_str(params, "content")?;
    let parent_id = get_optional_str(params, "parent_id");
    let comment = ops.create_comment(actor, post_id, content, parent_id).await.map_err(op_err)?;
    Ok(json!({ "data": comment }))
}

/// Get statistics for a project (counts by status).
async fn get_stats(params: &Value, store: &Store) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let project = queries(store).project(slug).await.map_err(op_err)?;

    let stats = store.project_stats(&project.id).await.map_err(|e| format!("Failed to compute stats: {e}"))?;
    let count = |status: &str| stats.by_status.get(status).copied().unwrap_or(0);

    Ok(json!({
        "total_posts": stats.total_posts,
        "open": count("open"),
        "planned": count("planned"),
        "in_progress": count("in_progress"),
        "done": count("done"),
        "declined": count("declined"),
        "total_votes": stats.total_votes,
    }))
}

/// Posts gaining votes fastest (7-day velocity).
async fn get_trending(params: &Value, store: &Store) -> Result<Value, McpError> {
    let limit = get_optional_u64(params, "limit").map(|n| n.min(i64::MAX as u64) as i64);
    let posts = queries(store).trending(get_str(params, "slug")?, limit).await.map_err(op_err)?;
    Ok(post_page(posts))
}

/// Aggregate analytics for a project (#186) — event totals + daily trend.
/// Privacy-first: counts only, no IP / user data exists in the source table.
async fn get_analytics(params: &Value, store: &Store) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let days = get_optional_u64(params, "days").unwrap_or(30).clamp(0, 365) as u32;

    let project = queries(store).project(slug).await.map_err(op_err)?;

    let totals =
        store.analytics_totals(&project.id, days).await.map_err(|e| format!("Failed to get analytics: {e}"))?;
    let daily =
        store.analytics_daily(&project.id, days).await.map_err(|e| format!("Failed to get daily analytics: {e}"))?;

    let daily_rows: Vec<Value> = daily
        .into_iter()
        .map(|(day, event_type, count)| json!({ "day": day, "event_type": event_type, "count": count }))
        .collect();

    Ok(json!({
        "data": {
            "project": slug,
            "days": days,
            "totals": totals,
            "daily": daily_rows,
        }
    }))
}

/// Top posts by views (#186) — with vote counts and vote/view conversion,
/// ready for AI-assisted prioritization ("which requests get attention but no votes?").
async fn get_top_posts(params: &Value, store: &Store) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let days = get_optional_u64(params, "days").unwrap_or(30).clamp(0, 365) as u32;
    let limit = get_optional_u64(params, "limit").unwrap_or(10).clamp(1, 50) as i64;

    let project = queries(store).project(slug).await.map_err(op_err)?;

    let top = store
        .analytics_top_posts(&project.id, days, limit)
        .await
        .map_err(|e| format!("Failed to get top posts: {e}"))?;

    let mut rows: Vec<Value> = Vec::with_capacity(top.len());
    for (post_id, views) in top {
        let (title, vote_count) = match store.get_post(&post_id, None).await {
            Ok(Some(detail)) => (detail.post.title, detail.post.vote_count),
            _ => (String::from("(deleted)"), 0),
        };
        let conversion = if views > 0 { (vote_count as f64 / views as f64 * 1000.0).round() / 10.0 } else { 0.0 };
        rows.push(json!({
            "post_id": post_id,
            "title": title,
            "views": views,
            "vote_count": vote_count,
            "vote_view_pct": conversion,
        }));
    }

    Ok(json!({ "data": rows }))
}

/// Create a project (admin parity — same rules as REST POST /api/projects).
async fn create_project(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let input = NewProject {
        name: get_str(params, "name")?,
        // An empty slug means "derive it from the name", as before.
        slug: get_optional_str(params, "slug").filter(|s| !s.is_empty()),
        description: get_optional_str(params, "description"),
    };
    let project = ops.create_project(actor, input).await.map_err(op_err)?;
    Ok(json!({ "data": project }))
}

/// Delete a project and everything in it, attachment files included
/// (admin parity — irreversible).
async fn delete_project(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let project = ops.delete_project(actor, slug).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": project.id, "slug": slug }))
}

/// List webhooks for a project (admin parity; secrets are NOT included).
async fn list_webhooks(params: &Value, store: &Store) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let project = queries(store).project(slug).await.map_err(op_err)?;
    let webhooks = store.list_webhooks(&project.id).await.map_err(|e| format!("Failed to list webhooks: {e}"))?;
    Ok(json!({ "data": webhooks, "total": webhooks.len() }))
}

/// Create a webhook subscription (admin parity — same URL rules as REST).
/// Auto-generates a signing secret when `secret` is omitted; the secret is
/// returned exactly once here.
async fn create_webhook(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let slug = get_str(params, "slug")?;
    let input = NewWebhook {
        url: get_str(params, "url")?,
        events: get_optional_str(params, "events"),
        secret: get_optional_str(params, "secret"),
    };
    let created = ops.create_webhook(actor, slug, input).await.map_err(op_err)?;
    Ok(json!({ "data": created.webhook, "signing_secret": created.secret }))
}

/// Delete a webhook (admin parity). An unknown id is an error; with the
/// optional `slug`, the webhook must also belong to that project.
async fn delete_webhook(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let id = get_str(params, "id")?;
    let slug = get_optional_str(params, "slug").filter(|s| !s.is_empty());
    ops.delete_webhook(actor, id, slug).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": id }))
}

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(ops: Operations) -> Result<()> {
    let actor = local_admin_actor(ops.store()).await?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let stdin = stdin.lock();
    let mut line = String::new();

    for result in stdin.lines() {
        line.clear();
        match result {
            Ok(l) => {
                // Guard against unbounded input lines
                if line.len() + l.len() > MAX_INPUT_LEN {
                    let _ = writeln!(
                        stdout,
                        "{}",
                        json!({
                            "jsonrpc": "2.0",
                            "error": {"code": -32600, "message": "Input too large (max 1MB)"},
                            "id": null
                        })
                    );
                    let _ = stdout.flush();
                    continue;
                }
                line.push_str(&l);
            }
            Err(_) => break,
        }

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_message(&line, &ops, &actor).await;
        if let Err(e) = writeln!(stdout, "{response}") {
            tracing::error!("Failed to write MCP response: {e}");
            break;
        }
        let _ = stdout.flush();
    }

    Ok(())
}

/// Delete a post by ID.
async fn delete_post(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let id = get_str(params, "id")?;
    ops.delete_post(actor, id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": id }))
}

/// Update a post's category.
async fn update_post_category(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let id = get_str(params, "id")?;
    let category_str = get_str(params, "category")?;
    ops.update_post(actor, id, PostChanges { status: None, category: Some(category_str) }).await.map_err(op_err)?;
    Ok(json!({ "updated": true, "id": id, "category": category_str }))
}

/// The roadmap: planned, in progress and done, most voted first.
async fn get_roadmap(params: &Value, store: &Store) -> Result<Value, McpError> {
    let per_bucket = get_optional_u64(params, "limit").map(|n| n.min(i64::MAX as u64) as i64);
    let roadmap = queries(store).roadmap(get_str(params, "slug")?, per_bucket).await.map_err(op_err)?;
    Ok(json!({
        "data": {
            "planned": roadmap.planned.posts,
            "planned_total": roadmap.planned.total,
            "in_progress": roadmap.in_progress.posts,
            "in_progress_total": roadmap.in_progress.total,
            "done": roadmap.done.posts,
            "done_total": roadmap.done.total,
            "limit": roadmap.limit,
        }
    }))
}

/// Delete an attachment by ID.
async fn delete_attachment(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let attachment_id = get_str(params, "id")?;
    ops.delete_attachment(actor, attachment_id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": attachment_id }))
}

/// Delete a comment by ID.
async fn delete_comment(params: &Value, ops: &Operations, actor: &Actor) -> Result<Value, McpError> {
    let comment_id = get_str(params, "comment_id")?;
    ops.delete_comment(actor, comment_id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "comment_id": comment_id }))
}

/// List attachments for a post.
async fn list_attachments(params: &Value, store: &Store) -> Result<Value, McpError> {
    let post_id = get_str(params, "post_id")?;
    // Verify post exists (consistent with REST API behavior)
    existing_post(store, post_id).await?;
    let attachments = store.list_attachments(post_id).await.map_err(|e| e.to_string())?;
    let total = attachments.len();
    Ok(json!({ "attachments": attachments, "total": total }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_ops() -> Operations {
        let pool = rungu_core::open_pool("sqlite::memory:").await.unwrap();
        rungu_core::run_migrations(&pool, "sqlite::memory:").await.unwrap();
        let store = Store::new_with_kind(pool, true);
        let storage = rungu_core::FsStorage::new(std::env::temp_dir().join("rungu-mcp-test-uploads")).unwrap();
        Operations::new(store, std::sync::Arc::new(storage), rungu_core::EventBus::default())
    }

    /// The stdio identity: the built-in local admin.
    async fn admin(ops: &Operations) -> Actor {
        local_admin_actor(ops.store()).await.unwrap()
    }

    async fn call_as(ops: &Operations, actor: &Actor, method: &str, params: Value) -> Value {
        let input = json!({ "jsonrpc": "2.0", "method": method, "params": params, "id": 1 }).to_string();
        serde_json::from_str(&handle_message(&input, ops, actor).await).unwrap()
    }

    async fn call(ops: &Operations, method: &str, params: Value) -> Value {
        call_as(ops, &admin(ops).await, method, params).await
    }

    /// #246: the admin tools now go through `Operations`, so they enforce the
    /// same rules as REST instead of their own weaker copies.
    #[tokio::test]
    async fn admin_tools_share_the_rest_rules() {
        let ops = test_ops().await;

        let bad_slug = call(&ops, "create_project", json!({ "name": "Board", "slug": "Has Spaces" })).await;
        assert!(bad_slug["error"]["message"].as_str().unwrap().contains("lowercase letters"));
        let made = call(&ops, "create_project", json!({ "name": "My Board!" })).await;
        assert_eq!(made["result"]["data"]["slug"], "my-board");
        let dup = call(&ops, "create_project", json!({ "name": "Again", "slug": "my-board" })).await;
        assert!(dup["error"]["message"].as_str().unwrap().contains("already exists"));

        let private = call(&ops, "create_webhook", json!({ "slug": "my-board", "url": "http://example.com/h" })).await;
        assert!(private["error"]["message"].as_str().unwrap().contains("HTTPS"));
        let hook = call(&ops, "create_webhook", json!({ "slug": "my-board", "url": "https://example.com/h" })).await;
        assert!(hook["result"]["signing_secret"].as_str().is_some_and(|s| !s.is_empty()));

        let missing = call(&ops, "delete_webhook", json!({ "id": "no-such-webhook" })).await;
        assert!(missing["error"]["message"].as_str().unwrap().contains("not found"));
        let id = hook["result"]["data"]["id"].clone();
        assert_eq!(call(&ops, "delete_webhook", json!({ "id": id })).await["result"]["deleted"], true);
    }

    /// #248: authorization comes from the transport's actor, so a non-admin
    /// actor is refused by `Operations` instead of being treated as admin.
    #[tokio::test]
    async fn tools_act_as_the_actor_the_transport_passes() {
        let ops = test_ops().await;
        let member = ops.store().find_or_create_user("member@x.test", None, None, &[]).await.unwrap();
        let member = Actor { id: member.id, role: UserRole::Member };

        let refused = call_as(&ops, &member, "create_project", json!({ "name": "Nope" })).await;
        assert_eq!(refused["error"]["code"], McpError::FORBIDDEN);
        assert_eq!(refused["error"]["data"]["kind"], "forbidden");
        assert!(ops.store().list_projects().await.unwrap().is_empty());

        // The same call as the stdio admin goes through.
        let made = call(&ops, "create_project", json!({ "name": "Yes" })).await;
        assert_eq!(made["result"]["data"]["slug"], "yes");

        // A member may act on their own posts, and the post carries their id.
        let post = call_as(&ops, &member, "create_post", json!({ "slug": "yes", "title": "Mine" })).await;
        assert_eq!(post["result"]["data"]["created_by"], member.id);
        let id = post["result"]["data"]["id"].clone();
        let other = ops.store().find_or_create_user("other@x.test", None, None, &[]).await.unwrap();
        let other = Actor { id: other.id, role: UserRole::Member };
        let denied = call_as(&ops, &other, "delete_post", json!({ "id": id })).await;
        assert_eq!(denied["error"]["code"], McpError::FORBIDDEN);
    }

    /// #248: each kind of failure has its own code, so clients can tell a
    /// typo from a missing thing from a server fault.
    #[tokio::test]
    async fn errors_carry_a_code_per_kind() {
        let ops = test_ops().await;
        call(&ops, "create_project", json!({ "name": "Acme" })).await;

        let code = |v: &Value| {
            (v["error"]["code"].as_i64().unwrap(), v["error"]["data"]["kind"].as_str().unwrap().to_string())
        };
        let missing_param = call(&ops, "get_project", json!({})).await;
        assert_eq!(code(&missing_param), (McpError::INVALID_PARAMS, "invalid_params".into()));
        let invalid = call(&ops, "list_posts", json!({ "slug": "acme", "status": "opennn" })).await;
        assert_eq!(code(&invalid), (McpError::INVALID_PARAMS, "invalid_params".into()));
        let not_found = call(&ops, "get_project", json!({ "slug": "nope" })).await;
        assert_eq!(code(&not_found), (McpError::NOT_FOUND, "not_found".into()));
        let conflict = call(&ops, "create_project", json!({ "name": "Again", "slug": "acme" })).await;
        assert_eq!(code(&conflict), (McpError::CONFLICT, "conflict".into()));
        let unknown = call(&ops, "no_such_tool", json!({})).await;
        assert_eq!(code(&unknown), (McpError::METHOD_NOT_FOUND, "method_not_found".into()));

        // The message is unchanged by the code.
        assert!(not_found["error"]["message"].as_str().unwrap().contains("not found"));

        // A missing post is a not-found on every tool that takes one.
        for (method, params) in [
            ("get_post", json!({ "id": "missing" })),
            ("list_comments", json!({ "post_id": "missing" })),
            ("list_attachments", json!({ "post_id": "missing" })),
        ] {
            let missing = call(&ops, method, params).await;
            assert_eq!(code(&missing), (McpError::NOT_FOUND, "not_found".into()), "{method}");
        }
    }

    /// Blank search is an error, not "the whole board" (shared with REST via BoardQuery).
    #[tokio::test]
    async fn search_posts_requires_a_query() {
        let ops = test_ops().await;
        call(&ops, "create_project", json!({ "name": "Acme" })).await;
        let blank = call(&ops, "search_posts", json!({ "slug": "acme", "q": "   " })).await;
        assert_eq!(blank["error"]["code"], McpError::INVALID_PARAMS);
    }

    /// A JSON-RPC notification (no id) runs the call but gets no response.
    #[tokio::test]
    async fn notifications_get_no_response() {
        let ops = test_ops().await;
        let input = json!({ "jsonrpc": "2.0", "method": "create_project", "params": { "name": "Quiet" } }).to_string();
        assert_eq!(handle_message(&input, &ops, &admin(&ops).await).await, "");
        assert!(ops.store().get_project_by_slug("quiet").await.unwrap().is_some(), "the side effect still ran");
    }

    #[tokio::test]
    async fn test_handle_message_invalid_json() {
        let ops = test_ops().await;
        let response = handle_message("not json", &ops, &admin(&ops).await).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn test_handle_message_unknown_method() {
        let ops = test_ops().await;
        let parsed = call(&ops, "nonexistent", json!({})).await;
        // Unknown methods are -32601 (Method not found) per JSON-RPC 2.0 (#190 scan).
        assert_eq!(parsed["error"]["code"], -32601);
        assert!(parsed["error"]["message"].as_str().unwrap().contains("Unknown method"));
    }

    #[tokio::test]
    async fn test_handle_message_too_large() {
        let ops = test_ops().await;
        let huge = "x".repeat(MAX_INPUT_LEN + 1);
        let response = handle_message(&huge, &ops, &admin(&ops).await).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
    }

    /// MCP shares the REST validation rules via `Operations`.
    #[tokio::test]
    async fn test_create_post_applies_shared_validation() {
        let ops = test_ops().await;
        ops.store().create_project("Acme", "acme", "").await.unwrap();

        let blank = call(&ops, "create_post", json!({ "slug": "acme", "title": "   " })).await;
        assert_eq!(blank["error"]["message"], "Title is required");

        let long = call(&ops, "create_post", json!({ "slug": "acme", "title": "x".repeat(201) })).await;
        assert_eq!(long["error"]["message"], "Title must be 200 characters or less");

        let bad_category =
            call(&ops, "create_post", json!({ "slug": "acme", "title": "Dark mode", "category": "nope" })).await;
        assert_eq!(bad_category["error"]["message"], "Invalid category");

        let ok = call(&ops, "create_post", json!({ "slug": "acme", "title": "  Dark mode  " })).await;
        assert_eq!(ok["result"]["data"]["title"], "Dark mode");
    }

    #[tokio::test]
    async fn test_mutations_on_missing_rows_are_not_found() {
        let ops = test_ops().await;
        let vote = call(&ops, "vote_post", json!({ "id": "missing" })).await;
        assert_eq!(vote["error"]["message"], "Post not found");
        let status = call(&ops, "update_post_status", json!({ "id": "missing", "status": "done" })).await;
        assert_eq!(status["error"]["message"], "Post not found");
        let comment = call(&ops, "delete_comment", json!({ "comment_id": "missing" })).await;
        assert_eq!(comment["error"]["message"], "Comment not found");
    }

    #[tokio::test]
    async fn test_get_stats_counts_every_post() {
        let ops = test_ops().await;
        ops.store().create_project("Acme", "acme", "").await.unwrap();
        for i in 0..3 {
            call(&ops, "create_post", json!({ "slug": "acme", "title": format!("Post {i}") })).await;
        }
        let stats = call(&ops, "get_stats", json!({ "slug": "acme" })).await;
        assert_eq!(stats["result"]["total_posts"], 3);
        assert_eq!(stats["result"]["open"], 3);
        assert_eq!(stats["result"]["done"], 0);
    }
}
