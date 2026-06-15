//! # rungu-mcp
//!
//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//! Spawned by AI agents as subprocess. No auth needed (process isolation).

use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::SqlitePool;

/// Process a single JSON-RPC message and return the response string.
pub async fn handle_message(input: &str, pool: &SqlitePool) -> String {
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
    let _params = msg.get("params").cloned().unwrap_or(json!({}));

    let result = handle_request(method, pool).await;

    match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }).to_string(),
        Err(code_msg) => json!({ "jsonrpc": "2.0", "error": {"code": -32603, "message": code_msg}, "id": id }).to_string(),
    }
}

async fn handle_request(method: &str, _pool: &SqlitePool) -> Result<Value, String> {
    match method {
        "list_projects" => Ok(json!({"projects": []})),
        "get_project" => Ok(json!({})),
        "list_posts" => Ok(json!({"posts": [], "total": 0})),
        "get_post" => Ok(json!({})),
        "create_post" => Ok(json!({"created": true})),
        "update_post_status" => Ok(json!({"updated": true})),
        "vote_post" => Ok(json!({"voted": true})),
        "search_posts" => Ok(json!({"posts": [], "total": 0})),
        "list_comments" => Ok(json!({"comments": []})),
        "add_comment" => Ok(json!({"created": true})),
        "get_stats" => Ok(json!({"total_posts": 0})),
        "get_trending" => Ok(json!({"posts": []})),
        _ => Err(format!("Unknown method: {method}")),
    }
}

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(pool: SqlitePool) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut stdin = stdin.lock();
    let mut line = String::new();

    for result in stdin.lines() {
        line.clear();
        match result {
            Ok(l) => line.push_str(&l),
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
