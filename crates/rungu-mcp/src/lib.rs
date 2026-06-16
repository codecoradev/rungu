//! # rungu-mcp
//!
//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//! Spawned by AI agents as subprocess. No auth needed (process isolation).

use std::io::{BufRead, Write};

use anyhow::Result;
use rungu_core::Store;
use rungu_proto::*;
use serde_json::{Value, json};
use sqlx::SqlitePool;

/// Maximum input line length (1 MB) to prevent unbounded memory usage.
const MAX_INPUT_LEN: usize = 1_048_576;

/// Process a single JSON-RPC message and return the response string.
pub async fn handle_message(input: &str, pool: &SqlitePool) -> String {
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
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));

    let store = Store::new(pool.clone());
    let result = handle_request(method, &params, &store).await;

    match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }).to_string(),
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
        "list_comments" => list_comments(params, store).await,
        "add_comment" => add_comment(params, store).await,
        "get_stats" => get_stats(params, store).await,
        "get_trending" => get_trending(params, store).await,
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
    Ok(json!({ "projects": projects }))
}

/// Get a single project by slug.
async fn get_project(params: &Value, store: &Store) -> Result<Value, String> {
    let slug = get_str(params, "slug")?;
    let project = store
        .get_project_by_slug(slug)
        .await
        .map_err(|e| format!("Failed to get project: {e}"))?
        .ok_or_else(|| format!("Project not found: {slug}"))?;
    Ok(json!({ "project": project }))
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
        .list_posts(ListPostsParams { project_id: &project.id, sort, status, category, query, offset: 0, limit })
        .await
        .map_err(|e| format!("Failed to list posts: {e}"))?;

    Ok(json!({ "posts": posts, "total": total }))
}

/// Get a single post by ID.
async fn get_post(params: &Value, store: &Store) -> Result<Value, String> {
    let id = get_str(params, "id")?;
    let post = store
        .get_post(id, None)
        .await
        .map_err(|e| format!("Failed to get post: {e}"))?
        .ok_or_else(|| format!("Post not found: {id}"))?;
    Ok(json!({ "post": post }))
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

    Ok(json!({ "post": post, "created": true }))
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

    Ok(json!({ "voted": voted, "vote_count": post.post.vote_count }))
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
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to search posts: {e}"))?;

    Ok(json!({ "posts": posts, "total": total }))
}

/// List comments for a post.
async fn list_comments(params: &Value, store: &Store) -> Result<Value, String> {
    let post_id = get_str(params, "post_id")?;
    let comments = store.list_comments(post_id).await.map_err(|e| format!("Failed to list comments: {e}"))?;

    Ok(json!({ "comments": comments }))
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

    Ok(json!({ "comment": comment, "created": true }))
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
            offset: 0,
            limit,
        })
        .await
        .map_err(|e| format!("Failed to get trending posts: {e}"))?;

    Ok(json!({ "posts": posts, "total": total }))
}

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(pool: SqlitePool) -> Result<()> {
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

        let response = handle_message(&line, &pool).await;
        if let Err(e) = writeln!(stdout, "{}", response) {
            tracing::error!("Failed to write MCP response: {e}");
            break;
        }
        let _ = stdout.flush();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_message_invalid_json() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let response = handle_message("not json", &pool).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn test_handle_message_unknown_method() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let input = r#"{"jsonrpc":"2.0","method":"nonexistent","id":1}"#;
        let response = handle_message(input, &pool).await;
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32603);
        assert!(parsed["error"]["message"].as_str().unwrap().contains("Unknown method"));
    }

    #[tokio::test]
    async fn test_handle_message_too_large() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let huge = "x".repeat(MAX_INPUT_LEN + 1);
        let response = handle_message(&huge, &pool).await;
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
