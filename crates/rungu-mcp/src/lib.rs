//! # rungu-mcp
//!
//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//!
//! ## ⚠️ Security model — local trusted subprocess ONLY
//!
//! This server runs **without authentication**. It is designed to be spawned as
//! a local subprocess of an AI agent (Claude Code, Cursor, Windsurf) on a
//! trusted single-user machine, with direct access to the SQLite database.
//!
//! This means:
//! - Any process that can spawn this binary can read and write all feedback data.
//! - There is **no row-level authorization** and **no user impersonation check**.
//! - Mutating tools (`create_post`, `update_post_status`, `update_post_category`,
//!   `delete_post`, `vote_post`, `add_comment`, `delete_comment`, `delete_attachment`)
//!   execute as the built-in `mcp@rungu.local` system user with the admin
//!   role, through the same [`Operations`] module as the REST API — so
//!   validation, storage cleanup, webhooks, and analytics behave identically.
//! - The HTTP API's OAuth/session/role model does **not** apply here.
//!
//! Never expose this server over the network, never share the database file
//! with untrusted users, and be aware that prompt-injection payloads can
//! instruct the host agent to call these tools. See
//! `docs/integrations/mcp.md#trust-boundary--security` for the full guidance.

use std::io::{BufRead, Write};

use anyhow::Result;
use rungu_core::ops::{NewPost, PostChanges, parse_category, parse_sort};
use rungu_core::{Actor, OpError, Operations, Store};
use rungu_proto::*;
use serde_json::{Value, json};

/// Maximum input line length (1 MB) to prevent unbounded memory usage.
const MAX_INPUT_LEN: usize = 1_048_576;

/// Process a single JSON-RPC message and return the response string.
pub async fn handle_message(input: &str, ops: &Operations) -> String {
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
        let _ = handle_request(&method, &params, ops).await;
        return String::new();
    }

    let result = handle_request(&method, &params, ops).await;

    match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }).to_string(),
        // Unknown tool/method is -32601 (Method not found); -32603 stays for
        // handler-internal failures.
        Err(msg) if msg.starts_with("Unknown") => {
            json!({ "jsonrpc": "2.0", "error": {"code": -32601, "message": msg}, "id": id }).to_string()
        }
        Err(msg) => json!({ "jsonrpc": "2.0", "error": {"code": -32603, "message": msg}, "id": id }).to_string(),
    }
}

/// Route a method to its handler with parsed params.
async fn handle_request(method: &str, params: &Value, ops: &Operations) -> Result<Value, String> {
    let store = ops.store();
    match method {
        "list_projects" => list_projects(store).await,
        "get_project" => get_project(params, store).await,
        "list_posts" => list_posts(params, store).await,
        "get_post" => get_post(params, store).await,
        "create_post" => create_post(params, ops).await,
        "update_post_status" => update_post_status(params, ops).await,
        "vote_post" => vote_post(params, ops).await,
        "search_posts" => search_posts(params, store).await,
        "get_changelog" => get_changelog(params, store).await,
        "list_comments" => list_comments(params, store).await,
        "add_comment" => add_comment(params, ops).await,
        "delete_comment" => delete_comment(params, ops).await,
        "get_stats" => get_stats(params, store).await,
        "get_trending" => get_trending(params, store).await,
        "get_analytics" => get_analytics(params, store).await,
        "get_top_posts" => get_top_posts(params, store).await,
        "create_project" => create_project(params, store).await,
        "delete_project" => delete_project(params, store).await,
        "list_webhooks" => list_webhooks(params, store).await,
        "create_webhook" => create_webhook(params, store).await,
        "delete_webhook" => delete_webhook(params, store).await,
        "list_attachments" => list_attachments(params, store).await,
        "delete_post" => delete_post(params, ops).await,
        "update_post_category" => update_post_category(params, ops).await,
        "get_roadmap" => get_roadmap(params, store).await,
        "delete_attachment" => delete_attachment(params, ops).await,
        _ => Err(format!("Unknown method: {method}")),
    }
}

// ── Param extraction helpers ───────────────────────────────────────────

fn get_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params.get(key).and_then(|v| v.as_str()).ok_or_else(|| format!("Missing required parameter: {key}"))
}

fn get_optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

fn get_optional_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(|v| v.as_u64())
}

fn parse_status(s: &str) -> Result<PostStatus, String> {
    rungu_core::ops::parse_status(s)
        .ok_or_else(|| format!("Invalid status: {s}. Must be one of: open, planned, in_progress, done, declined"))
}

fn parse_category_filter(s: &str) -> Result<PostCategory, String> {
    parse_category(s).ok_or_else(|| format!("Invalid category: {s}. Must be one of: feedback, bug, feature, question"))
}

/// Operation refusals become the JSON-RPC error message verbatim.
fn op_err(e: OpError) -> String {
    e.to_string()
}

/// Get or create the MCP system user for operations that need a user_id.
///
/// MCP runs as a trusted local admin (see the module docs).
async fn mcp_actor(store: &Store) -> Result<Actor, String> {
    let user = store
        .find_or_create_user("mcp@rungu.local", Some("MCP Bot"), None, &[])
        .await
        .map_err(|e| format!("Failed to get/create MCP user: {e}"))?;
    Ok(Actor { id: user.id, role: UserRole::Admin })
}

// ── Tool implementations ───────────────────────────────────────────────

/// List all projects.
async fn list_projects(store: &Store) -> Result<Value, String> {
    let projects = store.list_projects().await.map_err(|e| format!("Failed to list projects: {e}"))?;
    Ok(json!({ "data": projects }))
}

/// Get a single project by slug.
async fn get_project(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;
    Ok(json!({ "data": project }))
}

/// List posts in a project with optional filters.
async fn list_posts(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let sort = parse_sort(get_optional_str(params, "sort"));
    let status = get_optional_str(params, "status").map(parse_status).transpose()?;
    let category = get_optional_str(params, "category").map(parse_category_filter).transpose()?;
    let query = get_optional_str(params, "q");
    let limit = get_optional_u64(params, "limit").unwrap_or(20).clamp(1, 100) as i64;

    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort,
            status,
            category,
            query,
            since: None,
            user_id: None,
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to list posts: {e}"))?;

    Ok(json!({ "data": posts, "total": total }))
}

/// Get a single post by ID.
async fn get_post(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let post = store
        .get_post(id, None)
        .await
        .map_err(|e| format!("Failed to get post: {e}"))?
        .ok_or_else(|| format!("Post not found: {id}"))?;
    Ok(json!({ "data": post }))
}

/// Create a new post.
async fn create_post(params: &Value, ops: &Operations) -> Result<Value, String> {
    let input = NewPost {
        title: get_str(params, "title")?,
        description: get_optional_str(params, "description").unwrap_or(""),
        category: get_optional_str(params, "category"),
    };
    let actor = mcp_actor(ops.store()).await?;
    let post = ops.create_post(&actor, get_str(params, "slug")?, input).await.map_err(op_err)?;
    Ok(json!({ "data": post }))
}

/// Update a post's status.
async fn update_post_status(params: &Value, ops: &Operations) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let status_str = get_str(params, "status")?;
    let actor = mcp_actor(ops.store()).await?;
    ops.update_post(&actor, id, PostChanges { status: Some(status_str), category: None }).await.map_err(op_err)?;
    Ok(json!({ "updated": true, "id": id, "status": status_str }))
}

/// Toggle vote on a post.
async fn vote_post(params: &Value, ops: &Operations) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let actor = mcp_actor(ops.store()).await?;
    let outcome = ops.toggle_vote(&actor, id).await.map_err(op_err)?;
    Ok(json!({ "data": { "voted": outcome.voted, "vote_count": outcome.vote_count } }))
}

/// Search posts by query string.
async fn search_posts(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let query = get_str(params, "q")?;
    let limit = get_optional_u64(params, "limit").unwrap_or(20).clamp(1, 100) as i64;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: Some(query),
            since: None,
            user_id: None,
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to search posts: {e}"))?;

    Ok(json!({ "data": posts, "total": total }))
}

/// Get the changelog for a project — done posts, most recently shipped first.
///
/// Useful for AI agents summarizing "what shipped recently" in a project.
async fn get_changelog(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let limit = get_optional_u64(params, "limit").unwrap_or(20).clamp(1, 100) as i64;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::RecentlyUpdated,
            status: Some(PostStatus::Done),
            category: None,
            query: None,
            since: None,
            user_id: None,
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to get changelog: {e}"))?;

    Ok(json!({ "data": posts, "total": total }))
}

/// List comments for a post.
async fn list_comments(params: &Value, store: &Store) -> Result<Value, String> {
    let post_id = get_str(params, "post_id")?;
    let comments = store.list_comments(post_id).await.map_err(|e| format!("Failed to list comments: {e}"))?;

    Ok(json!({ "data": comments }))
}

/// Add a comment to a post.
async fn add_comment(params: &Value, ops: &Operations) -> Result<Value, String> {
    let post_id = get_str(params, "post_id")?;
    let content = get_str(params, "content")?;
    let parent_id = get_optional_str(params, "parent_id");
    let actor = mcp_actor(ops.store()).await?;
    let comment = ops.create_comment(&actor, post_id, content, parent_id).await.map_err(op_err)?;
    Ok(json!({ "data": comment }))
}

/// Get statistics for a project (counts by status).
async fn get_stats(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

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

/// Get trending posts (sorted by most votes).
async fn get_trending(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let limit = get_optional_u64(params, "limit").unwrap_or(10).clamp(1, 100) as i64;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            // Real trending (#203): 7-day vote velocity, tie-broken by totals.
            sort: PostSort::Trending,
            status: None,
            category: None,
            query: None,
            since: None,
            user_id: None,
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to get trending posts: {e}"))?;

    Ok(json!({ "data": posts, "total": total }))
}

/// Aggregate analytics for a project (#186) — event totals + daily trend.
/// Privacy-first: counts only, no IP / user data exists in the source table.
async fn get_analytics(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let days = get_optional_u64(params, "days").unwrap_or(30).clamp(0, 365) as u32;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

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
async fn get_top_posts(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let days = get_optional_u64(params, "days").unwrap_or(30).clamp(0, 365) as u32;
    let limit = get_optional_u64(params, "limit").unwrap_or(10).clamp(1, 50) as i64;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

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

/// Create a project (admin parity — mirrors REST POST /api/projects).
async fn create_project(params: &Value, store: &Store) -> Result<Value, String> {
    let name = get_str(params, "name")?;
    let slug = get_optional_str(params, "slug").unwrap_or("");
    let description = get_optional_str(params, "description").unwrap_or("");
    let slug = if slug.is_empty() { name.to_lowercase().replace(' ', "-") } else { slug.to_string() };

    let project =
        store.create_project(name, &slug, description).await.map_err(|e| format!("Failed to create project: {e}"))?;
    Ok(json!({ "data": project }))
}

/// Delete a project and everything in it (admin parity — irreversible).
async fn delete_project(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;
    store.delete_project(&project.id).await.map_err(|e| format!("Failed to delete project: {e}"))?;
    Ok(json!({ "deleted": true, "id": project.id, "slug": slug }))
}

/// List webhooks for a project (admin parity; secrets are NOT included).
async fn list_webhooks(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;
    let webhooks = store.list_webhooks(&project.id).await.map_err(|e| format!("Failed to list webhooks: {e}"))?;
    Ok(json!({ "data": webhooks, "total": webhooks.len() }))
}

/// Create a webhook subscription (admin parity). Auto-generates a signing
/// secret when `secret` is omitted; the secret is returned exactly once here.
async fn create_webhook(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let url = get_str(params, "url")?;
    let events = get_optional_str(params, "events").unwrap_or("*");
    let secret = match get_optional_str(params, "secret") {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => uuid::Uuid::new_v4().to_string(),
    };

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;
    let webhook = store
        .create_webhook(&project.id, url, events, &secret)
        .await
        .map_err(|e| format!("Failed to create webhook: {e}"))?;
    Ok(json!({ "data": webhook, "signing_secret": secret }))
}

/// Delete a webhook (admin parity).
async fn delete_webhook(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    store.delete_webhook(id).await.map_err(|e| format!("Failed to delete webhook: {e}"))?;
    Ok(json!({ "deleted": true, "id": id }))
}

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(ops: Operations) -> Result<()> {
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

        let response = handle_message(&line, &ops).await;
        if let Err(e) = writeln!(stdout, "{response}") {
            tracing::error!("Failed to write MCP response: {e}");
            break;
        }
        let _ = stdout.flush();
    }

    Ok(())
}

/// Delete a post by ID.
async fn delete_post(params: &Value, ops: &Operations) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let actor = mcp_actor(ops.store()).await?;
    ops.delete_post(&actor, id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": id }))
}

/// Update a post's category.
async fn update_post_category(params: &Value, ops: &Operations) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let category_str = get_str(params, "category")?;
    let actor = mcp_actor(ops.store()).await?;
    ops.update_post(&actor, id, PostChanges { status: None, category: Some(category_str) }).await.map_err(op_err)?;
    Ok(json!({ "updated": true, "id": id, "category": category_str }))
}

/// Get a project roadmap — posts grouped by lifecycle status (planned, in_progress, done).
///
/// Mirrors the REST `GET /projects/{slug}/roadmap` endpoint.
async fn get_roadmap(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let per_bucket = get_optional_u64(params, "limit").unwrap_or(10).clamp(1, 50) as i64;

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let planned = fetch_roadmap_bucket(store, &project.id, PostStatus::Planned, per_bucket).await?;
    let in_progress = fetch_roadmap_bucket(store, &project.id, PostStatus::InProgress, per_bucket).await?;
    let done = fetch_roadmap_bucket(store, &project.id, PostStatus::Done, per_bucket).await?;

    Ok(json!({
        "data": {
            "planned": planned.0,
            "planned_total": planned.1,
            "in_progress": in_progress.0,
            "in_progress_total": in_progress.1,
            "done": done.0,
            "done_total": done.1,
            "limit": per_bucket,
        }
    }))
}

/// Helper: fetch a single roadmap bucket (top-N most-voted posts for a status).
async fn fetch_roadmap_bucket(
    store: &Store,
    project_id: &str,
    status: PostStatus,
    limit: i64,
) -> Result<(Vec<PostDetail>, i64), String> {
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id,
            sort: PostSort::MostVotes,
            status: Some(status),
            category: None,
            query: None,
            since: None,
            user_id: None,
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to fetch roadmap bucket: {e}"))?;
    Ok((posts, total))
}

/// Delete an attachment by ID.
async fn delete_attachment(params: &Value, ops: &Operations) -> Result<Value, String> {
    let attachment_id = get_str(params, "id")?;
    let actor = mcp_actor(ops.store()).await?;
    ops.delete_attachment(&actor, attachment_id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "id": attachment_id }))
}

/// Delete a comment by ID.
async fn delete_comment(params: &Value, ops: &Operations) -> Result<Value, String> {
    let comment_id = get_str(params, "comment_id")?;
    let actor = mcp_actor(ops.store()).await?;
    ops.delete_comment(&actor, comment_id).await.map_err(op_err)?;
    Ok(json!({ "deleted": true, "comment_id": comment_id }))
}

/// List attachments for a post.
async fn list_attachments(params: &Value, store: &Store) -> Result<Value, String> {
    let post_id = get_str(params, "post_id")?;
    // Verify post exists (consistent with REST API behavior)
    store
        .get_post(post_id, None)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Post not found: {post_id}"))?;
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

    async fn call(ops: &Operations, method: &str, params: Value) -> Value {
        let input = json!({ "jsonrpc": "2.0", "method": method, "params": params, "id": 1 }).to_string();
        serde_json::from_str(&handle_message(&input, ops).await).unwrap()
    }

    #[tokio::test]
    async fn test_handle_message_invalid_json() {
        let ops = test_ops().await;
        let response = handle_message("not json", &ops).await;
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
        let response = handle_message(&huge, &ops).await;
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

    #[test]
    fn test_parse_status() {
        assert!(parse_status("in_progress").is_ok());
        assert!(parse_status("invalid").is_err());
    }

    #[test]
    fn test_parse_category_filter_is_strict() {
        assert!(matches!(parse_category_filter("bug"), Ok(PostCategory::Bug)));
        assert!(parse_category_filter("unknown").is_err());
    }
}
