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
//!   execute as the built-in `mcp@rungu.local` system user.
//! - The HTTP API's OAuth/session/role model does **not** apply here.
//!
//! Never expose this server over the network, never share the database file
//! with untrusted users, and be aware that prompt-injection payloads can
//! instruct the host agent to call these tools. See
//! `docs/integrations/mcp.md#trust-boundary--security` for the full guidance.

use std::io::{BufRead, Write};

use anyhow::Result;
use rungu_core::Store;
use rungu_proto::*;
use serde_json::{Value, json};
use sqlx::AnyPool;

/// Maximum input line length (1 MB) to prevent unbounded memory usage.
const MAX_INPUT_LEN: usize = 1_048_576;

/// Process a single JSON-RPC message and return the response string.
pub async fn handle_message(input: &str, pool: &AnyPool, is_sqlite: bool) -> String {
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
        let store = Store::new_with_kind(pool.clone(), is_sqlite);
        let _ = handle_request(&method, &params, &store).await;
        return String::new();
    }

    let store = Store::new_with_kind(pool.clone(), is_sqlite);
    let result = handle_request(&method, &params, &store).await;

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
async fn handle_request(method: &str, params: &Value, store: &Store) -> Result<Value, String> {
    match method {
        "list_projects" => list_projects(store).await,
        "get_project" => get_project(params, store).await,
        "list_posts" => list_posts(params, store).await,
        "get_post" => get_post(params, store).await,
        "create_post" => create_post(params, store).await,
        "update_post_status" => update_post_status(params, store).await,
        "vote_post" => vote_post(params, store).await,
        "search_posts" => search_posts(params, store).await,
        "get_changelog" => get_changelog(params, store).await,
        "list_comments" => list_comments(params, store).await,
        "add_comment" => add_comment(params, store).await,
        "delete_comment" => delete_comment(params, store).await,
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
        "delete_post" => delete_post(params, store).await,
        "update_post_category" => update_post_category(params, store).await,
        "get_roadmap" => get_roadmap(params, store).await,
        "delete_attachment" => delete_attachment(params, store).await,
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
    match s {
        "open" => Ok(PostStatus::Open),
        "planned" => Ok(PostStatus::Planned),
        "in_progress" => Ok(PostStatus::InProgress),
        "done" => Ok(PostStatus::Done),
        "declined" => Ok(PostStatus::Declined),
        _ => Err(format!("Invalid status: {s}. Must be one of: open, planned, in_progress, done, declined")),
    }
}

fn parse_category(s: &str) -> PostCategory {
    match s {
        "bug" => PostCategory::Bug,
        "feature" => PostCategory::Feature,
        "question" => PostCategory::Question,
        _ => PostCategory::Feedback,
    }
}

/// Strict variant for update paths: an unknown category is a client error,
/// not something to silently coerce to "feedback" (#190 scan).
fn parse_category_strict(s: &str) -> Option<PostCategory> {
    match s {
        "bug" => Some(PostCategory::Bug),
        "feature" => Some(PostCategory::Feature),
        "question" => Some(PostCategory::Question),
        "feedback" => Some(PostCategory::Feedback),
        _ => None,
    }
}

fn parse_sort(s: Option<&str>) -> PostSort {
    match s {
        Some("oldest") => PostSort::Oldest,
        Some("most_votes") => PostSort::MostVotes,
        Some("least_votes") => PostSort::LeastVotes,
        Some("recently_updated") => PostSort::RecentlyUpdated,
        _ => PostSort::Newest,
    }
}

/// Get or create the MCP system user for operations that need a user_id.
async fn get_mcp_user(store: &Store) -> Result<String, String> {
    let user = store
        .find_or_create_user("mcp@rungu.local", Some("MCP Bot"), None, &[])
        .await
        .map_err(|e| format!("Failed to get/create MCP user: {e}"))?;
    Ok(user.id)
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
    let category = get_optional_str(params, "category").map(parse_category);
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
async fn create_post(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let title = get_str(params, "title")?;
    let description = get_optional_str(params, "description").unwrap_or("");
    let category = get_optional_str(params, "category").map(parse_category).unwrap_or(PostCategory::Feedback);

    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;

    let user_id = get_mcp_user(store).await?;

    let post = store
        .create_post(&project.id, title, description, category, &user_id)
        .await
        .map_err(|e| format!("Failed to create post: {e}"))?;

    Ok(json!({ "data": post }))
}

/// Update a post's status.
async fn update_post_status(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let status_str = get_str(params, "status")?;
    let status = parse_status(status_str)?;

    store.update_post_status(id, status).await.map_err(|e| format!("Failed to update post status: {e}"))?;

    Ok(json!({ "updated": true, "id": id, "status": status_str }))
}

/// Toggle vote on a post.
async fn vote_post(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let user_id = get_mcp_user(store).await?;

    let voted = store.toggle_vote(&user_id, id).await.map_err(|e| format!("Failed to toggle vote: {e}"))?;

    let post = store
        .get_post(id, None)
        .await
        .map_err(|e| format!("Failed to get post after vote: {e}"))?
        .ok_or("Post not found after vote")?;

    Ok(json!({ "data": { "voted": voted, "vote_count": post.post.vote_count } }))
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
async fn add_comment(params: &Value, store: &Store) -> Result<Value, String> {
    let post_id = get_str(params, "post_id")?;
    let content = get_str(params, "content")?;
    let parent_id = get_optional_str(params, "parent_id");
    let user_id = get_mcp_user(store).await?;

    let comment = store
        .create_comment(post_id, content, parent_id, &user_id)
        .await
        .map_err(|e| format!("Failed to create comment: {e}"))?;

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

    // Fetch all posts (up to 1000 for stats)
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            user_id: None,
            offset: 0,
            limit: 1000,
        })
        .await
        .map_err(|e| format!("Failed to fetch posts for stats: {e}"))?;

    let mut open = 0u64;
    let mut planned = 0u64;
    let mut in_progress = 0u64;
    let mut done = 0u64;
    let mut declined = 0u64;

    for p in &posts {
        match p.post.status {
            PostStatus::Open => open += 1,
            PostStatus::Planned => planned += 1,
            PostStatus::InProgress => in_progress += 1,
            PostStatus::Done => done += 1,
            PostStatus::Declined => declined += 1,
        }
    }

    let total_votes: i64 = posts.iter().map(|p| p.post.vote_count).sum();

    Ok(json!({
        "total_posts": total,
        "open": open,
        "planned": planned,
        "in_progress": in_progress,
        "done": done,
        "declined": declined,
        "total_votes": total_votes,
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
            sort: PostSort::MostVotes,
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
pub async fn run_server(pool: AnyPool, is_sqlite: bool) -> Result<()> {
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

        let response = handle_message(&line, &pool, is_sqlite).await;
        if let Err(e) = writeln!(stdout, "{response}") {
            tracing::error!("Failed to write MCP response: {e}");
            break;
        }
        let _ = stdout.flush();
    }

    Ok(())
}

/// Delete a post by ID.
async fn delete_post(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    store.delete_post(id).await.map_err(|e| format!("Failed to delete post: {e}"))?;
    Ok(json!({ "deleted": true, "id": id }))
}

/// Update a post's category.
async fn update_post_category(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let category_str = get_str(params, "category")?;
    let category = parse_category_strict(category_str).ok_or_else(|| format!("Unknown category: {category_str}"))?;

    store.update_post_category(id, category).await.map_err(|e| format!("Failed to update post category: {e}"))?;

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
async fn delete_attachment(params: &Value, store: &Store) -> Result<Value, String> {
    let attachment_id = get_str(params, "id")?;
    store.delete_attachment(attachment_id).await.map_err(|e| format!("Failed to delete attachment: {e}"))?;
    Ok(json!({ "deleted": true, "id": attachment_id }))
}

/// Delete a comment by ID.
async fn delete_comment(params: &Value, store: &Store) -> Result<Value, String> {
    let comment_id = get_str(params, "comment_id")?;
    store.delete_comment(comment_id).await.map_err(|e| e.to_string())?;
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

    #[tokio::test]
    async fn test_handle_message_invalid_json() {
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();
        let response = handle_message("not json", &pool, true).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn test_handle_message_unknown_method() {
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();
        let input = r#"{"jsonrpc":"2.0","method":"nonexistent","id":1}"#;
        let response = handle_message(input, &pool, true).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        // Unknown methods are -32601 (Method not found) per JSON-RPC 2.0 (#190 scan).
        assert_eq!(parsed["error"]["code"], -32601);
        assert!(parsed["error"]["message"].as_str().unwrap().contains("Unknown method"));
    }

    #[tokio::test]
    async fn test_handle_message_too_large() {
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();
        let huge = "x".repeat(MAX_INPUT_LEN + 1);
        let response = handle_message(&huge, &pool, true).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
    }

    #[tokio::test]
    async fn test_parse_status_valid() {
        assert!(parse_status("open").is_ok());
        assert!(parse_status("planned").is_ok());
        assert!(parse_status("in_progress").is_ok());
        assert!(parse_status("done").is_ok());
        assert!(parse_status("declined").is_ok());
    }

    #[tokio::test]
    async fn test_parse_status_invalid() {
        assert!(parse_status("invalid").is_err());
    }

    #[tokio::test]
    async fn test_parse_category() {
        assert!(matches!(parse_category("bug"), PostCategory::Bug));
        assert!(matches!(parse_category("feature"), PostCategory::Feature));
        assert!(matches!(parse_category("question"), PostCategory::Question));
        assert!(matches!(parse_category("unknown"), PostCategory::Feedback));
    }

    #[tokio::test]
    async fn test_parse_sort() {
        assert!(matches!(parse_sort(None), PostSort::Newest));
        assert!(matches!(parse_sort(Some("oldest")), PostSort::Oldest));
        assert!(matches!(parse_sort(Some("most_votes")), PostSort::MostVotes));
    }
}
