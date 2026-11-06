//! The catalog of MCP tools: one declaration per capability.
//!
//! Each [`Tool`] states everything about a capability in one place: its name,
//! summary, parameters, who may call it, what it does to data, which REST
//! operation does the same thing, and the handler. The rest is derived:
//!
//! - **dispatch** looks a tool up here, so a tool without a declaration can't
//!   exist;
//! - **`tools/list`** is generated from the declarations ([`tools_list`]);
//! - **REST parity** is enforced by `rungu-api`'s `parity_test`: every OpenAPI
//!   operation must be claimed by a tool's [`RestRoute`] or listed in
//!   [`REST_ONLY`] with a reason, and the access level must agree with the
//!   route's security. A new endpoint or tool that skips this fails CI.
//!
//! To add a capability, see "Adding a capability" in
//! `docs/development/architecture.md`.

use std::future::Future;
use std::pin::Pin;

use rungu_core::{Actor, Operations, Store};
use serde_json::{Value, json};

use crate::McpError;
use crate::{
    add_comment, create_post, create_project, create_webhook, delete_attachment, delete_comment, delete_post,
    delete_project, delete_webhook, get_analytics, get_changelog, get_post, get_project, get_roadmap, get_stats,
    get_top_posts, get_trending, list_attachments, list_comments, list_posts, list_projects, list_webhooks,
    search_posts, update_post_category, update_post_status, vote_post,
};

type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, McpError>> + Send + 'a>>;

/// How a tool runs.
enum Handler {
    /// Reads through the store; needs no identity.
    Read(for<'a> fn(&'a Value, &'a Store) -> ToolFuture<'a>),
    /// Reads through the store, but the answer depends on who is asking.
    ReadAs(for<'a> fn(&'a Value, &'a Store, &'a Actor) -> ToolFuture<'a>),
    /// Goes through [`Operations`] as the actor the transport authenticated.
    Write(for<'a> fn(&'a Value, &'a Operations, &'a Actor) -> ToolFuture<'a>),
}

macro_rules! read {
    ($f:ident) => {
        Handler::Read(|p, s| Box::pin($f(p, s)))
    };
}
macro_rules! read_no_params {
    ($f:ident) => {
        Handler::Read(|_, s| Box::pin($f(s)))
    };
}
macro_rules! read_as {
    ($f:ident) => {
        Handler::ReadAs(|p, s, a| Box::pin($f(p, s, a)))
    };
}
macro_rules! write {
    ($f:ident) => {
        Handler::Write(|p, o, a| Box::pin($f(p, o, a)))
    };
}

/// JSON type of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Integer,
}

/// One input parameter.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    pub description: &'static str,
}

const fn req(name: &'static str, description: &'static str) -> Param {
    Param { name, kind: Kind::String, required: true, description }
}
const fn opt(name: &'static str, description: &'static str) -> Param {
    Param { name, kind: Kind::String, required: false, description }
}
const fn opt_int(name: &'static str, description: &'static str) -> Param {
    Param { name, kind: Kind::Integer, required: false, description }
}

/// Who the REST API lets perform the capability. `Public` means no login.
/// MCP itself acts as the transport's actor (an admin today); this is the
/// level the shared rules in `Operations` enforce, and what the parity test
/// checks against the REST route's security.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Public,
    Member,
    OwnerOrAdmin,
    Admin,
}

/// What a call does to data. Drives the `readOnlyHint` / `destructiveHint`
/// annotations clients use to decide whether to ask before running a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Read,
    Write,
    Destructive,
}

/// The REST operation that does the same thing, as OpenAPI names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestRoute {
    pub method: &'static str,
    pub path: &'static str,
}

const fn rest(method: &'static str, path: &'static str) -> RestRoute {
    RestRoute { method, path }
}

/// A capability exposed over MCP.
pub struct Tool {
    pub name: &'static str,
    pub summary: &'static str,
    pub params: &'static [Param],
    pub access: Access,
    pub effect: Effect,
    /// The REST operation with the same behaviour.
    pub rest: RestRoute,
    handler: Handler,
}

impl Tool {
    /// Run the tool. Reads need no identity; writes act as `actor`.
    ///
    /// The declared [`Access`] is enforced here for the one level that reads
    /// can't leave to `Operations`: admin-only tools refuse a non-admin actor
    /// before running, whether they read or write. (`Member` and
    /// `OwnerOrAdmin` writes are checked by `Operations`, which knows the
    /// post, comment or attachment owner.)
    pub(crate) fn call<'a>(&self, params: &'a Value, ops: &'a Operations, actor: &'a Actor) -> ToolFuture<'a> {
        if self.access == Access::Admin && !actor.is_admin() {
            return Box::pin(async {
                Err(McpError::from(rungu_core::OpError::Forbidden("Admin access required".into())))
            });
        }
        if let Err(e) = self.check_params(params) {
            return Box::pin(async { Err(e) });
        }
        match &self.handler {
            Handler::Read(f) => f(params, ops.store()),
            Handler::ReadAs(f) => f(params, ops.store(), actor),
            Handler::Write(f) => f(params, ops, actor),
        }
    }

    /// Reject arguments whose JSON type contradicts the declared schema. The
    /// handlers read leniently (`"5"` for an integer would be dropped as if
    /// absent), so a client that sent the wrong type would silently get the
    /// default instead of an error. `null` counts as absent.
    fn check_params(&self, params: &Value) -> Result<(), McpError> {
        for p in self.params {
            let Some(v) = params.get(p.name).filter(|v| !v.is_null()) else { continue };
            let ok = match p.kind {
                Kind::String => v.is_string(),
                Kind::Integer => v.is_u64(),
            };
            if !ok {
                let expected = match p.kind {
                    Kind::String => "a string",
                    Kind::Integer => "a non-negative integer",
                };
                return Err(McpError::invalid_params(format!("Parameter `{}` must be {expected}", p.name)));
            }
        }
        Ok(())
    }

    /// The tool as an MCP `tools/list` entry.
    pub fn to_json(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for p in self.params {
            let ty = match p.kind {
                Kind::String => "string",
                Kind::Integer => "integer",
            };
            properties.insert(p.name.to_string(), json!({ "type": ty, "description": p.description }));
            if p.required {
                required.push(p.name);
            }
        }
        json!({
            "name": self.name,
            "description": self.summary,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": {
                "readOnlyHint": self.effect == Effect::Read,
                "destructiveHint": self.effect == Effect::Destructive,
                // Every tool only touches this Rungu instance.
                "openWorldHint": false,
            },
        })
    }
}

/// Look a tool up by name.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The result of `tools/list`: every tool, generated from [`TOOLS`].
pub fn tools_list() -> Value {
    json!({ "tools": TOOLS.iter().map(Tool::to_json).collect::<Vec<_>>() })
}

const SLUG: Param = req("slug", "Project slug");
const PAGE: Param = opt_int("page", "Page number, starting at 1");
const LIMIT: Param = opt_int("limit", "Page size (max 100)");

/// Every MCP tool, in the order `tools/list` returns them.
pub static TOOLS: &[Tool] = &[
    // ── Projects ────────────────────────────────────────────────────
    Tool {
        name: "list_projects",
        summary: "List all feedback projects",
        params: &[],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects"),
        handler: read_no_params!(list_projects),
    },
    Tool {
        name: "get_project",
        summary: "Get project detail by slug",
        params: &[SLUG],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}"),
        handler: read!(get_project),
    },
    // ── Posts ───────────────────────────────────────────────────────
    Tool {
        name: "list_posts",
        summary: "List posts with filters (status, category, sort, q) and paging. Same rules and results as GET /api/projects/{slug}/posts",
        params: &[
            SLUG,
            opt("status", "open, planned, in_progress, done or declined"),
            opt("category", "feedback, bug, feature or question"),
            opt("sort", "newest, oldest, most_votes, least_votes, recently_updated or trending"),
            opt("q", "Full-text search"),
            PAGE,
            LIMIT,
        ],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/posts"),
        handler: read!(list_posts),
    },
    Tool {
        name: "get_post",
        summary: "Get post detail",
        params: &[req("id", "Post ID")],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/posts/{id}"),
        handler: read_as!(get_post),
    },
    Tool {
        name: "create_post",
        summary: "Submit a new feedback post",
        params: &[
            SLUG,
            req("title", "Post title, up to 200 characters"),
            opt("description", "Post body"),
            opt("category", "feedback (default), bug, feature or question"),
        ],
        access: Access::Member,
        effect: Effect::Write,
        rest: rest("POST", "/api/projects/{slug}/posts"),
        handler: write!(create_post),
    },
    Tool {
        name: "update_post_status",
        summary: "Change post status",
        params: &[req("id", "Post ID"), req("status", "open, planned, in_progress, done or declined")],
        access: Access::OwnerOrAdmin,
        effect: Effect::Write,
        rest: rest("PATCH", "/api/posts/{id}"),
        handler: write!(update_post_status),
    },
    Tool {
        name: "update_post_category",
        summary: "Change post category",
        params: &[req("id", "Post ID"), req("category", "feedback, bug, feature or question")],
        access: Access::OwnerOrAdmin,
        effect: Effect::Write,
        rest: rest("PATCH", "/api/posts/{id}"),
        handler: write!(update_post_category),
    },
    Tool {
        name: "delete_post",
        summary: "Delete a post by ID, with its attachments",
        params: &[req("id", "Post ID")],
        access: Access::OwnerOrAdmin,
        effect: Effect::Destructive,
        rest: rest("DELETE", "/api/posts/{id}"),
        handler: write!(delete_post),
    },
    Tool {
        name: "vote_post",
        summary: "Toggle vote on a post",
        params: &[req("id", "Post ID")],
        access: Access::Member,
        effect: Effect::Write,
        rest: rest("POST", "/api/posts/{id}/vote"),
        handler: write!(vote_post),
    },
    Tool {
        name: "search_posts",
        summary: "Full-text search across posts, newest first",
        params: &[SLUG, req("q", "Search text"), PAGE, LIMIT],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/posts"),
        handler: read!(search_posts),
    },
    Tool {
        name: "get_roadmap",
        summary: "Posts grouped by lifecycle status (planned, in_progress, done), most voted first",
        params: &[SLUG, opt_int("limit", "Posts per bucket (default 10, max 50)")],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/roadmap"),
        handler: read!(get_roadmap),
    },
    Tool {
        name: "get_changelog",
        summary: "Shipped (done) posts, most recently updated first",
        params: &[SLUG, opt("since", "Only posts updated since this RFC 3339 timestamp"), PAGE, LIMIT],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/changelog"),
        handler: read!(get_changelog),
    },
    Tool {
        name: "get_trending",
        summary: "Posts gaining votes fastest over the last 7 days",
        params: &[SLUG, opt_int("limit", "Number of posts (default 10, max 100)")],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/posts"),
        handler: read!(get_trending),
    },
    // ── Comments ────────────────────────────────────────────────────
    Tool {
        name: "list_comments",
        summary: "Get comments for a post",
        params: &[req("post_id", "Post ID")],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/posts/{id}/comments"),
        handler: read!(list_comments),
    },
    Tool {
        name: "add_comment",
        summary: "Add comment to a post",
        params: &[
            req("post_id", "Post ID"),
            req("content", "Comment text, up to 4000 characters"),
            opt("parent_id", "Comment to reply to"),
        ],
        access: Access::Member,
        effect: Effect::Write,
        rest: rest("POST", "/api/posts/{id}/comments"),
        handler: write!(add_comment),
    },
    Tool {
        name: "delete_comment",
        summary: "Delete a comment by ID",
        params: &[req("comment_id", "Comment ID")],
        access: Access::OwnerOrAdmin,
        effect: Effect::Destructive,
        rest: rest("DELETE", "/api/comments/{id}"),
        handler: write!(delete_comment),
    },
    // ── Attachments ─────────────────────────────────────────────────
    Tool {
        name: "list_attachments",
        summary: "List image attachments for a post",
        params: &[req("post_id", "Post ID")],
        access: Access::Public,
        effect: Effect::Read,
        rest: rest("GET", "/api/posts/{id}/attachments"),
        handler: read!(list_attachments),
    },
    Tool {
        name: "delete_attachment",
        summary: "Delete an attachment by ID",
        params: &[req("id", "Attachment ID")],
        access: Access::OwnerOrAdmin,
        effect: Effect::Destructive,
        rest: rest("DELETE", "/api/attachments/{id}"),
        handler: write!(delete_attachment),
    },
    // ── Insights ────────────────────────────────────────────────────
    Tool {
        name: "get_stats",
        summary: "Project stats (total posts, by status, votes)",
        params: &[SLUG],
        access: Access::Admin,
        effect: Effect::Read,
        rest: rest("GET", "/api/admin/projects/{slug}/stats"),
        handler: read!(get_stats),
    },
    Tool {
        name: "get_analytics",
        summary: "Event totals and daily trend (views, votes, comments): privacy-first aggregates",
        params: &[SLUG, opt_int("days", "Window in days (default 30, max 365; 0 = all time)")],
        access: Access::Admin,
        effect: Effect::Read,
        rest: rest("GET", "/api/admin/analytics/{slug}"),
        handler: read!(get_analytics),
    },
    Tool {
        name: "get_top_posts",
        summary: "Top posts by views with vote/view conversion (for prioritisation)",
        params: &[
            SLUG,
            opt_int("days", "Window in days (default 30, max 365; 0 = all time)"),
            opt_int("limit", "Number of posts (default 10, max 50)"),
        ],
        access: Access::Admin,
        effect: Effect::Read,
        rest: rest("GET", "/api/admin/analytics/{slug}/top"),
        handler: read!(get_top_posts),
    },
    // ── Admin ───────────────────────────────────────────────────────
    Tool {
        name: "create_project",
        summary: "Create a new feedback project. Same name and slug rules as the REST API",
        params: &[
            req("name", "Project name, up to 100 characters"),
            opt("slug", "URL slug (derived from the name if omitted)"),
            opt("description", "Short description"),
        ],
        access: Access::Admin,
        effect: Effect::Write,
        rest: rest("POST", "/api/projects"),
        handler: write!(create_project),
    },
    Tool {
        name: "delete_project",
        summary: "Delete a project and everything in it, attachment files included. Irreversible",
        params: &[SLUG],
        access: Access::Admin,
        effect: Effect::Destructive,
        rest: rest("DELETE", "/api/projects/{slug}"),
        handler: write!(delete_project),
    },
    Tool {
        name: "list_webhooks",
        summary: "List webhooks for a project (secrets are not included)",
        params: &[SLUG],
        access: Access::Admin,
        effect: Effect::Read,
        rest: rest("GET", "/api/projects/{slug}/webhooks"),
        handler: read!(list_webhooks),
    },
    Tool {
        name: "create_webhook",
        summary: "Create a webhook subscription; the signing secret is returned once. Same URL rules as the REST API",
        params: &[
            SLUG,
            req("url", "HTTPS target URL"),
            opt("events", "Comma-separated event names (default: all)"),
            opt("secret", "Signing secret (generated if omitted)"),
        ],
        access: Access::Admin,
        effect: Effect::Write,
        rest: rest("POST", "/api/projects/{slug}/webhooks"),
        handler: write!(create_webhook),
    },
    Tool {
        name: "delete_webhook",
        summary: "Delete a webhook subscription by id; an optional slug also checks it belongs to that project",
        params: &[req("id", "Webhook ID"), opt("slug", "Project slug the webhook must belong to")],
        access: Access::Admin,
        effect: Effect::Destructive,
        rest: rest("DELETE", "/api/projects/{slug}/webhooks/{id}"),
        handler: write!(delete_webhook),
    },
];

/// REST operations that have no MCP tool, and why. Together with the routes
/// in [`TOOLS`] this must cover every operation in the OpenAPI document
/// (`rungu-api`'s `parity_test` checks it), so a new endpoint has to be
/// claimed by a tool or listed here on purpose.
///
/// Entries reading "candidate" are useful to agents and not yet exposed.
pub const REST_ONLY: &[(&str, &str, &str)] = &[
    ("GET", "/api/admin/posts", "moderation queue for the web admin UI"),
    ("POST", "/api/posts/{id}/attachments", "multipart upload; MCP carries JSON, not file bodies"),
    ("GET", "/api/attachments/{id}", "binary file download"),
    ("GET", "/auth/providers", "browser login; MCP clients authenticate at the transport"),
    ("GET", "/auth/{provider}/login", "browser login; MCP clients authenticate at the transport"),
    ("GET", "/auth/{provider}/callback", "browser login; MCP clients authenticate at the transport"),
    ("POST", "/auth/logout", "browser session"),
    ("GET", "/auth/me", "browser session"),
    ("GET", "/api/meta", "instance branding for the web UI"),
    ("GET", "/api/me/notifications/preferences", "per-user setting; MCP acts as a system user"),
    ("POST", "/api/me/notifications/preferences", "per-user setting; MCP acts as a system user"),
    ("GET", "/api/notifications/unsubscribe", "email unsubscribe page, opened from a mail link"),
    ("POST", "/api/notifications/unsubscribe", "email unsubscribe action, opened from a mail link"),
    ("GET", "/api/posts/{id}/official-response", "candidate for an MCP tool"),
    ("PUT", "/api/posts/{id}/official-response", "candidate for an MCP tool"),
    ("GET", "/api/posts/{id}/similar", "candidate for an MCP tool"),
    ("GET", "/api/projects/{slug}/similar", "candidate for an MCP tool"),
    ("GET", "/api/projects/{slug}/counts", "sidebar counts for the web UI; get_stats covers the data"),
    ("PATCH", "/api/projects/{slug}", "candidate for an MCP tool (update_project)"),
    ("GET", "/api/posts/{id}/vote", "whether the viewer has voted; web UI state"),
    ("GET", "/api/projects/{slug}/webhooks/{id}", "candidate for an MCP tool"),
    ("PATCH", "/api/projects/{slug}/webhooks/{id}", "candidate for an MCP tool"),
    ("POST", "/api/projects/{slug}/webhooks/{id}/test", "candidate for an MCP tool"),
    ("GET", "/api/projects/{slug}/webhooks/{id}/deliveries", "candidate for an MCP tool"),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn names_are_unique_and_snake_case() {
        let mut seen = HashSet::new();
        for tool in TOOLS {
            assert!(seen.insert(tool.name), "duplicate tool {}", tool.name);
            assert!(tool.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", tool.name);
            assert!(!tool.summary.is_empty(), "{} needs a summary", tool.name);
        }
    }

    #[test]
    fn params_are_well_formed() {
        for tool in TOOLS {
            let mut names = HashSet::new();
            for p in tool.params {
                assert!(names.insert(p.name), "{}: duplicate param {}", tool.name, p.name);
                assert!(!p.description.is_empty(), "{}.{} needs a description", tool.name, p.name);
            }
            // Every {placeholder} in the REST path is something the tool takes
            // (as itself, or as `id`/`post_id`/`comment_id` for the resource).
            for placeholder in tool.rest.path.split('{').skip(1).map(|s| s.split('}').next().unwrap()) {
                let known = match placeholder {
                    "slug" => tool.params.iter().any(|p| p.name == "slug"),
                    _ => tool.params.iter().any(|p| matches!(p.name, "id" | "post_id" | "comment_id")),
                };
                assert!(known, "{}: REST path takes {{{placeholder}}} but the tool has no matching param", tool.name);
            }
        }
    }

    #[test]
    fn destructive_tools_are_exactly_the_deletes() {
        for tool in TOOLS {
            assert_eq!(
                tool.effect == Effect::Destructive,
                tool.name.starts_with("delete_"),
                "{}: effect and name disagree",
                tool.name
            );
            assert_eq!(tool.effect == Effect::Read, tool.rest.method == "GET", "{}: effect and REST method", tool.name);
        }
    }

    #[test]
    fn tools_list_describes_every_tool() {
        let list = tools_list();
        let tools = list["tools"].as_array().unwrap();
        assert_eq!(tools.len(), TOOLS.len());
        let create = tools.iter().find(|t| t["name"] == "create_post").unwrap();
        assert_eq!(create["inputSchema"]["type"], "object");
        assert_eq!(create["inputSchema"]["required"], json!(["slug", "title"]));
        assert_eq!(create["inputSchema"]["properties"]["title"]["type"], "string");
        assert_eq!(create["annotations"]["readOnlyHint"], false);
        let trending = tools.iter().find(|t| t["name"] == "get_trending").unwrap();
        assert_eq!(trending["inputSchema"]["properties"]["limit"]["type"], "integer");
        assert_eq!(trending["annotations"]["readOnlyHint"], true);
        let delete = tools.iter().find(|t| t["name"] == "delete_project").unwrap();
        assert_eq!(delete["annotations"]["destructiveHint"], true);
        assert!(tools.iter().all(|t| t["annotations"]["openWorldHint"] == false));
    }

    #[test]
    fn rest_only_entries_have_reasons_and_no_overlap_with_tools() {
        for (method, path, reason) in REST_ONLY {
            assert!(!reason.is_empty(), "{method} {path} needs a reason");
            assert!(
                !TOOLS.iter().any(|t| t.rest.method == *method && t.rest.path == *path),
                "{method} {path} is both a tool's route and REST-only"
            );
        }
        let unique: HashSet<_> = REST_ONLY.iter().map(|(m, p, _)| (*m, *p)).collect();
        assert_eq!(unique.len(), REST_ONLY.len(), "duplicate REST_ONLY entry");
    }

    /// The docs name every tool and state the right count, so adding a tool
    /// without documenting it (or leaving a stale "12 tools") fails here.
    #[test]
    fn docs_list_every_tool_and_the_right_count() {
        let mcp_doc = include_str!("../../../docs/integrations/mcp.md");
        for tool in TOOLS {
            assert!(
                mcp_doc.contains(&format!("`{}`", tool.name)),
                "docs/integrations/mcp.md doesn't list `{}`",
                tool.name
            );
        }

        let n = TOOLS.len().to_string();
        let counted = [
            ("README.md", include_str!("../../../README.md")),
            ("docs/configuration.md", include_str!("../../../docs/configuration.md")),
            ("docs/integrations/mcp.md", mcp_doc),
            ("docs/development/architecture.md", include_str!("../../../docs/development/architecture.md")),
        ];
        let digits = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit());
        for (file, text) in counted {
            for line in text.lines() {
                let words: Vec<&str> = line.split_whitespace().collect();
                for (i, word) in words.iter().enumerate() {
                    if !word.starts_with("tools") {
                        continue;
                    }
                    // "26 tools", "26 AI agent tools": a number up to three words before.
                    for count in words[i.saturating_sub(3)..i].iter().filter(|w| digits(w)) {
                        assert_eq!(*count, n, "{file} says \"{count} ... tools\" but there are {n}: {line}");
                    }
                }
            }
        }
    }
}
