//! # rungu-proto
//!
//! Wire types and protocol definitions for Rungu.
//! Shared across all crates — no business logic here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Enums ────────────────────────────────────────────────────────────────

/// Post status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    Open,
    Planned,
    InProgress,
    Done,
    Declined,
}

impl Default for PostStatus {
    fn default() -> Self {
        Self::Open
    }
}

/// Post category type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostCategory {
    Feedback,
    Bug,
    Feature,
    Question,
}

impl Default for PostCategory {
    fn default() -> Self {
        Self::Feedback
    }
}

/// User role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Member,
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Member
    }
}

/// OAuth provider name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    Google,
    GitHub,
    Keycloak,
}

// ── User ────────────────────────────────────────────────────────────────

/// User record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar_url: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
}

/// OAuth identity linked to a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    pub id: String,
    pub user_id: String,
    pub provider: AuthProvider,
    pub provider_id: String,
    pub created_at: DateTime<Utc>,
}

/// Current authenticated user (from JWT session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub id: String,
    pub email: String,
    pub role: UserRole,
}

// ── Project ───────────────────────────────────────────────────────────────

/// Project record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

// ── Post ─────────────────────────────────────────────────────────────────

/// Post (feedback/bug/feature/question).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub status: PostStatus,
    pub category: PostCategory,
    pub vote_count: i64,
    pub comment_count: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Post detail (includes creator info).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostDetail {
    #[serde(flatten)]
    pub post: Post,
    pub creator: UserSummary,
    pub user_voted: bool,
}

/// Lightweight user info for post creator display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar_url: String,
}

// ── Vote ─────────────────────────────────────────────────────────────────

/// Vote record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub user_id: String,
    pub post_id: String,
    pub created_at: DateTime<Utc>,
}

// ── Comment ──────────────────────────────────────────────────────────────

/// Comment record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub post_id: String,
    pub parent_id: Option<String>,
    pub content: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

/// Comment detail (includes creator info).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentDetail {
    #[serde(flatten)]
    pub comment: Comment,
    pub creator: UserSummary,
}

// ── API Request/Response ─────────────────────────────────────────────────

/// Create post request.
#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub category: PostCategory,
}

/// Create comment request.
#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub parent_id: Option<String>,
}

/// Update post status request (admin).
#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: PostStatus,
}

/// Create project request (admin).
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: Option<String>,
    #[serde(default)]
    pub description: String,
}

/// Generic list response with pagination.
#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// Sort options for posts.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostSort {
    Newest,
    Oldest,
    MostVotes,
    LeastVotes,
    RecentlyUpdated,
}

impl Default for PostSort {
    fn default() -> Self {
        Self::Newest
    }
}

/// Query params for listing posts.
#[derive(Debug, Deserialize)]
pub struct ListPostsQuery {
    pub sort: Option<PostSort>,
    pub status: Option<PostStatus>,
    pub category: Option<PostCategory>,
    pub q: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

// ── Stats ───────────────────────────────────────────────────────────────

/// Project statistics.
#[derive(Debug, Serialize)]
pub struct ProjectStats {
    pub total_posts: i64,
    pub by_status: std::collections::HashMap<String, i64>,
    pub by_category: std::collections::HashMap<String, i64>,
    pub total_users: i64,
}

/// OAuth identity returned from provider.
#[derive(Debug, Clone)]
pub struct OAuthIdentity {
    pub provider_id: String,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

// ── Auth Provider Info (for frontend login buttons) ──────────────────────

/// Active auth provider info (sent to frontend).
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub login_url: String,
}
