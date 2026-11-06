//! # store
//!
//! SQLite storage operations for Rungu.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rungu_proto::*;
use sqlx::{AnyPool, Row, any::AnyRow};

/// Parse a timestamp string from the DB (with fallback).
fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Parse a just-generated RFC3339 timestamp (infallible for our own `to_rfc3339()`).
fn parse_now(s: &str) -> DateTime<Utc> {
    // now_ts() always produces valid RFC3339, so this is truly infallible.
    DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now())
}

/// Parse UserRole from SQLite TEXT column.
fn parse_role(s: &str) -> UserRole {
    match s {
        "admin" => UserRole::Admin,
        _ => UserRole::Member,
    }
}

/// Map a SQLite row to a Project.
fn map_project(row: &AnyRow) -> Project {
    Project {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        description: row.get("description"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
    }
}

/// Map a SQLite row to a User.
fn map_user(row: &AnyRow) -> User {
    User {
        id: row.get("id"),
        email: row.get("email"),
        name: row.get("name"),
        avatar_url: row.get("avatar_url"),
        role: parse_role(row.get::<&str, _>("role")),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
        last_login: parse_ts(row.get::<&str, _>("last_login")),
    }
}

/// Map a SQLite row to a PostDetail (with user join + vote status).
fn map_post_detail(row: &AnyRow) -> PostDetail {
    let post = map_post(row);
    let creator =
        UserSummary { id: row.get("user_id"), name: row.get("user_name"), avatar_url: row.get("user_avatar") };
    PostDetail { post, creator, user_voted: false }
}

/// Map a SQLite row to a Post.
fn map_post(row: &AnyRow) -> Post {
    Post {
        id: row.get("id"),
        project_id: row.get("project_id"),
        title: row.get("title"),
        description: row.get("description"),
        status: parse_status(row.get::<&str, _>("status")),
        category: parse_category(row.get::<&str, _>("category")),
        vote_count: row.get("vote_count"),
        comment_count: row.get("comment_count"),
        created_by: row.get("created_by"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
        updated_at: parse_ts(row.get::<&str, _>("updated_at")),
    }
}

/// Map a SQLite row to a Comment.
fn map_comment(row: &AnyRow) -> Comment {
    Comment {
        id: row.get("id"),
        post_id: row.get("post_id"),
        parent_id: row.get("parent_id"),
        content: row.get("content"),
        created_by: row.get("created_by"),
        created_at: parse_ts(row.get::<&str, _>("created_at")),
    }
}

/// Map a SQLite row to a CommentDetail (with user join).
fn map_comment_detail(row: &AnyRow) -> CommentDetail {
    let comment = map_comment(row);
    let creator =
        UserSummary { id: row.get("user_id"), name: row.get("user_name"), avatar_url: row.get("user_avatar") };
    CommentDetail { comment, creator }
}

/// Convert PostStatus to DB string (avoid format! Debug for underscore variants).
fn status_to_str(s: PostStatus) -> &'static str {
    match s {
        PostStatus::Open => "open",
        PostStatus::Planned => "planned",
        PostStatus::InProgress => "in_progress",
        PostStatus::Done => "done",
        PostStatus::Declined => "declined",
    }
}

/// Convert PostCategory to DB string.
fn category_to_str(c: PostCategory) -> &'static str {
    match c {
        PostCategory::Feedback => "feedback",
        PostCategory::Bug => "bug",
        PostCategory::Feature => "feature",
        PostCategory::Question => "question",
    }
}

/// Filler words common in feedback titles that say nothing about the topic
/// ("Please add support for ..."). Only words of 4+ characters matter, since
/// shorter ones are dropped anyway.
const SIMILARITY_STOPWORDS: &[&str] = &[
    "please", "would", "could", "should", "there", "their", "them", "they", "this", "that", "these", "those", "with",
    "without", "from", "into", "have", "having", "when", "what", "which", "where", "while", "will", "just", "like",
    "also", "some", "more", "very", "really", "able", "allow", "allows", "being", "been", "does", "make", "need",
    "want", "support", "supports", "feature", "option", "issue", "thing", "things", "using",
];

/// Keywords used to find posts similar to `title`: words of 4+ characters,
/// lowercased, minus filler words, deduplicated, longest four first.
fn similarity_tokens(title: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut tokens: Vec<String> = title
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|t| t.chars().count() >= 4 && !SIMILARITY_STOPWORDS.contains(&t.as_str()))
        .filter(|t| seen.insert(t.clone()))
        .collect();
    // Stable sort: equal-length words keep their title order.
    tokens.sort_by_key(|t| std::cmp::Reverse(t.chars().count()));
    tokens.truncate(4);
    tokens
}

/// Sanitize a user-supplied search term into a safe FTS5 MATCH expression.
///
/// FTS5 query syntax ("AND", "OR", "NEAR", `*`, `^`, `"..."`, column filters)
/// would let a crafted query do expensive work or syntax-error out. To keep
/// behavior predictable and safe, we split the input on whitespace and turn
/// each token into a prefix-match token (`token*`), then join with implicit
/// AND (space-separated). Empty/whitespace input yields an empty string,
/// which the caller treats as "no MATCH".
///
/// Example: `"dark mode"` → `"dark* mode*"`.
/// Sanitize a single FTS5 token: keep alphanumerics/underscore/hyphen, drop
/// FTS5-special characters. Returns None when nothing alphanumeric remains.
fn sanitize_fts_token(tok: &str) -> Option<String> {
    if !tok.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }
    let cleaned: String = tok.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

fn sanitize_fts_query(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split_whitespace()
        .filter_map(sanitize_fts_token)
        .map(|cleaned| format!("{cleaned}*"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Which full-text search path `list_posts` should use, if any.
///
/// Kept as a small enum (rather than two `Option`s) so the JOIN, WHERE
/// fragment, and bind value all branch on a single decision.
enum Search {
    /// SQLite FTS5: bind a sanitized prefix-token MATCH expression.
    Fts5(String),
    /// PostgreSQL: bind the raw query to `plainto_tsquery` against the
    /// generated `search_tsv` column.
    PgTsv(String),
    /// No search filter applied.
    None,
}

/// Parse PostStatus from SQLite TEXT column.
fn parse_status(s: &str) -> PostStatus {
    match s {
        "planned" => PostStatus::Planned,
        "in_progress" => PostStatus::InProgress,
        "done" => PostStatus::Done,
        "declined" => PostStatus::Declined,
        _ => PostStatus::Open,
    }
}

/// Parse PostCategory from SQLite TEXT column.
fn parse_category(s: &str) -> PostCategory {
    match s {
        "bug" => PostCategory::Bug,
        "feature" => PostCategory::Feature,
        "question" => PostCategory::Question,
        _ => PostCategory::Feedback,
    }
}

/// Storage layer — all database operations.
#[derive(Clone)]
pub struct Store {
    pool: AnyPool,
    /// True when the underlying database is SQLite. Drives FTS5 vs PostgreSQL
    /// `tsvector` search-path selection (SQLite has the `posts_fts` virtual
    /// table populated by triggers; PostgreSQL uses a generated `tsvector`
    /// column + GIN index).
    is_sqlite: bool,
    /// Short-TTL cache for `get_project_by_slug`. Project slugs are resolved
    /// on nearly every board/vote/comment request, but rarely change, so a
    /// small cache eliminates most of those round-trips. Mutations to projects
    /// (`create`/`update`/`delete`) invalidate the whole cache — projects are
    /// few and the simplicity beats per-key tracking.
    project_cache: moka::future::Cache<String, Project>,
}

/// How long a cached `Project` row stays fresh before re-hitting the DB.
const PROJECT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// Upper bound on cached projects. Projects are few in practice; this just
/// caps memory if a caller enumerates many slugs (including misses).
const PROJECT_CACHE_MAX_ENTRIES: u64 = 256;

impl Store {
    pub fn new(pool: AnyPool) -> Self {
        Self::build(pool, false)
    }

    /// Construct a Store with explicit backend knowledge.
    ///
    /// `is_sqlite` should be `true` when the pool was opened against a
    /// `sqlite:` connection. `lib::open_pool` already detects this when the
    /// URL starts with `sqlite:` — callers should pass that detection result
    /// here so the store can pick the right query dialect.
    pub fn new_with_kind(pool: AnyPool, is_sqlite: bool) -> Self {
        Self::build(pool, is_sqlite)
    }

    fn build(pool: AnyPool, is_sqlite: bool) -> Self {
        let project_cache = moka::future::Cache::builder()
            .time_to_live(PROJECT_CACHE_TTL)
            .max_capacity(PROJECT_CACHE_MAX_ENTRIES)
            .build();
        Self { pool, is_sqlite, project_cache }
    }

    /// Get a reference to the pool.
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }
}

mod analytics;
mod attachments;
mod comments;
mod notifications;
mod posts;
mod projects;
mod users;
mod webhooks;

pub use notifications::NotifyUser;

#[cfg(test)]
mod similarity_tests {
    use super::similarity_tokens;

    #[test]
    fn drops_filler_and_short_words() {
        assert_eq!(similarity_tokens("Please add support for dark mode"), vec!["dark", "mode"]);
    }

    #[test]
    fn lowercases_dedups_and_strips_punctuation() {
        assert_eq!(similarity_tokens("Export, export EXPORT to CSV!"), vec!["export"]);
    }

    #[test]
    fn keeps_the_four_longest_words_in_title_order_on_ties() {
        assert_eq!(
            similarity_tokens("webhook retries timeout signature payload"),
            vec!["signature", "webhook", "retries", "timeout"]
        );
    }

    #[test]
    fn empty_when_nothing_meaningful_remains() {
        assert!(similarity_tokens("a b c please").is_empty());
    }
}
