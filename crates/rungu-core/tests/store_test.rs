//! Integration tests for the Store layer against in-memory SQLite.
//!
//! Tests verify: CRUD operations, filters, cascading deletes,
//! SQL injection prevention in search, and edge cases.

use rungu_core::{Store, open_pool, run_migrations};
use rungu_proto::*;

async fn setup() -> Store {
    let pool = open_pool("sqlite::memory:").await.unwrap();
    run_migrations(&pool, "sqlite::memory:").await.unwrap();
    Store::new_with_kind(pool, true)
}

// ── Projects ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_project_crud() {
    let store = setup().await;

    // Create
    let project = store.create_project("My App", "my-app", "Test project").await.unwrap();
    assert_eq!(project.name, "My App");
    assert_eq!(project.slug, "my-app");

    // Get by slug
    let found = store.get_project_by_slug("my-app").await.unwrap().unwrap();
    assert_eq!(found.id, project.id);

    // Get by id
    let found = store.get_project_by_id(&project.id).await.unwrap().unwrap();
    assert_eq!(found.name, "My App");

    // List
    let all = store.list_projects().await.unwrap();
    assert_eq!(all.len(), 1);

    // Update
    let updated = store.update_project(&project.id, Some("Updated Name"), None).await.unwrap();
    assert_eq!(updated.name, "Updated Name");

    // Delete
    store.delete_project(&project.id).await.unwrap();
    assert!(store.get_project_by_slug("my-app").await.unwrap().is_none());
}

#[tokio::test]
async fn test_project_delete_cascades() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("test@test.com", None, None, &[]).await.unwrap();
    let post = store.create_post(&project.id, "Title", "Desc", PostCategory::Feedback, &user.id).await.unwrap();

    // Delete project should cascade-delete posts
    store.delete_project(&project.id).await.unwrap();
    assert!(store.get_post(&post.id, None).await.unwrap().is_none());
}

// ── Posts ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_post_crud() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("user@test.com", None, None, &[]).await.unwrap();

    // Create
    let post =
        store.create_post(&project.id, "Feature X", "Add feature X", PostCategory::Feature, &user.id).await.unwrap();
    assert_eq!(post.title, "Feature X");
    assert_eq!(post.status, PostStatus::Open);
    assert_eq!(post.vote_count, 0);

    // Get
    let found = store.get_post(&post.id, None).await.unwrap().unwrap();
    assert_eq!(found.post.title, "Feature X");
    assert!(!found.user_voted);

    // Update status
    store.update_post_status(&post.id, PostStatus::Done).await.unwrap();
    let updated = store.get_post(&post.id, None).await.unwrap().unwrap();
    assert_eq!(updated.post.status, PostStatus::Done);

    // Delete
    store.delete_post(&post.id).await.unwrap();
    assert!(store.get_post(&post.id, None).await.unwrap().is_none());
}

#[tokio::test]
async fn test_list_posts_with_filters() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("user@test.com", None, None, &[]).await.unwrap();

    // Create varied posts
    let _p1 =
        store.create_post(&project.id, "Bug report", "crash on startup", PostCategory::Bug, &user.id).await.unwrap();
    let p2 = store
        .create_post(&project.id, "Feature request", "add dark mode", PostCategory::Feature, &user.id)
        .await
        .unwrap();
    store.update_post_status(&p2.id, PostStatus::Done).await.unwrap();
    let p3 =
        store.create_post(&project.id, "Question", "how to export?", PostCategory::Question, &user.id).await.unwrap();

    // List all
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(posts.len(), 3);

    // Filter by category
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: Some(PostCategory::Bug),
            query: None,
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(posts[0].post.title, "Bug report");

    // Filter by status
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: Some(PostStatus::Done),
            category: None,
            query: None,
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(posts[0].post.title, "Feature request");

    // Search query
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: Some("dark"),
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(posts[0].post.title, "Feature request");

    let _ = p3;
}

#[tokio::test]
async fn test_list_posts_search_with_sql_injection_chars() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();

    store.create_post(&project.id, "Normal Post", "content", PostCategory::Feedback, &user.id).await.unwrap();

    // SQL injection attempt — should return 0 results, not crash or dump data
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: Some("'; DROP TABLE posts; --"),
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 0);
    assert!(posts.is_empty());

    // Verify table still exists
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(posts.len(), 1);
}

#[tokio::test]
async fn test_list_posts_search_fts5_multitoken() {
    // The store uses SQLite's FTS5 virtual table (`posts_fts`, maintained by
    // triggers) for search when the backend is SQLite. A multi-token query
    // must match a post that contains ALL tokens (implicit AND), not just one.
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();

    store
        .create_post(
            &project.id,
            "Dark mode for the dashboard",
            "Add a dark theme that follows system preference",
            PostCategory::Feature,
            &user.id,
        )
        .await
        .unwrap();
    // Distractor: has "dark" but not "mode".
    store
        .create_post(
            &project.id,
            "Fix dark flicker on load",
            "Background flashes before CSS loads",
            PostCategory::Bug,
            &user.id,
        )
        .await
        .unwrap();

    // Multi-token query → only the post with BOTH "dark" AND "mode" matches.
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: Some("dark mode"),
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    assert_eq!(total, 1, "multi-token FTS5 query should AND the tokens");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].post.title, "Dark mode for the dashboard");
}

#[tokio::test]
async fn test_list_posts_search_punctuation_only_drops_to_noop() {
    // A query that is only punctuation/operators must not produce a broken
    // FTS5 MATCH expression. The store sanitizes it to empty and treats it
    // as "no search" — so it returns ALL posts for the project.
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    store.create_post(&project.id, "Hello", "world", PostCategory::Feedback, &user.id).await.unwrap();

    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: Some("!!!"),
            since: None,
            offset: 0,
            limit: 20,
        })
        .await
        .unwrap();
    // Punctuation-only → sanitized to empty → no MATCH clause → returns the one post.
    assert_eq!(total, 1);
    assert_eq!(posts.len(), 1);
}

#[tokio::test]
async fn test_list_posts_pagination() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();

    // Create 5 posts
    for i in 0..5 {
        store.create_post(&project.id, &format!("Post {i}"), "", PostCategory::Feedback, &user.id).await.unwrap();
    }

    // Page 1 (2 per page)
    let (posts, total) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            offset: 0,
            limit: 2,
        })
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(posts.len(), 2);

    // Page 3 (beyond data)
    let (posts, _) = store
        .list_posts(ListPostsParams {
            project_id: &project.id,
            sort: PostSort::Newest,
            status: None,
            category: None,
            query: None,
            since: None,
            offset: 4,
            limit: 2,
        })
        .await
        .unwrap();
    assert_eq!(posts.len(), 1); // only 1 remaining
}

// ── Votes ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_vote_toggle() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let post = store.create_post(&project.id, "Title", "", PostCategory::Feedback, &user.id).await.unwrap();

    // Vote
    let voted = store.toggle_vote(&user.id, &post.id).await.unwrap();
    assert!(voted);
    assert!(store.has_voted(&user.id, &post.id).await.unwrap());

    // Check vote count incremented
    let detail = store.get_post(&post.id, Some(&user.id)).await.unwrap().unwrap();
    assert_eq!(detail.post.vote_count, 1);
    assert!(detail.user_voted);

    // Unvote (idempotent toggle)
    let voted = store.toggle_vote(&user.id, &post.id).await.unwrap();
    assert!(!voted);
    assert!(!store.has_voted(&user.id, &post.id).await.unwrap());

    let detail = store.get_post(&post.id, Some(&user.id)).await.unwrap().unwrap();
    assert_eq!(detail.post.vote_count, 0);
    assert!(!detail.user_voted);
}

#[tokio::test]
async fn test_delete_post_cascades_votes_and_comments() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let post = store.create_post(&project.id, "Title", "", PostCategory::Feedback, &user.id).await.unwrap();

    // Add vote + comment
    store.toggle_vote(&user.id, &post.id).await.unwrap();
    store.create_comment(&post.id, "Nice post", None, &user.id).await.unwrap();

    // Delete post → should cascade
    store.delete_post(&post.id).await.unwrap();
    assert!(store.get_post(&post.id, None).await.unwrap().is_none());
    assert!(store.list_comments(&post.id).await.unwrap().is_empty());
}

// ── Comments ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_comment_crud_and_threading() {
    let store = setup().await;
    let project = store.create_project("App", "app", "").await.unwrap();
    let user = store.find_or_create_user("u@t.com", None, None, &[]).await.unwrap();
    let post = store.create_post(&project.id, "Title", "", PostCategory::Feedback, &user.id).await.unwrap();

    // Create top-level comment
    let c1 = store.create_comment(&post.id, "First comment", None, &user.id).await.unwrap();
    assert_eq!(c1.comment.content, "First comment");
    assert!(c1.comment.parent_id.is_none());

    // Create reply
    let c2 = store.create_comment(&post.id, "Reply to first", Some(&c1.comment.id), &user.id).await.unwrap();
    assert_eq!(c2.comment.parent_id.as_deref(), Some(c1.comment.id.as_str()));

    // List comments
    let comments = store.list_comments(&post.id).await.unwrap();
    assert_eq!(comments.len(), 2);

    // Get single comment
    let found = store.get_comment(&c1.comment.id).await.unwrap().unwrap();
    assert_eq!(found.content, "First comment");

    // Delete comment
    store.delete_comment(&c1.comment.id).await.unwrap();
    assert!(store.get_comment(&c1.comment.id).await.unwrap().is_none());
}

// ── Users ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_user_find_or_create_email_dedup() {
    let store = setup().await;

    // Create new user
    let u1 = store.find_or_create_user("test@example.com", Some("Alice"), None, &[]).await.unwrap();
    assert_eq!(u1.email, "test@example.com");
    assert_eq!(u1.name, "Alice");
    assert_eq!(u1.role, UserRole::Member);

    // Same email → should return same user (dedup)
    let u2 = store.find_or_create_user("test@example.com", Some("Alice Updated"), None, &[]).await.unwrap();
    assert_eq!(u1.id, u2.id);

    // Different email → different user
    let u3 = store.find_or_create_user("other@example.com", None, None, &[]).await.unwrap();
    assert_ne!(u1.id, u3.id);

    // upsert identity link
    store.upsert_identity(&u1.id, "google", "google-123").await.unwrap();
    store.upsert_identity(&u1.id, "github", "github-456").await.unwrap();
    // Duplicate identity should be a no-op (ON CONFLICT DO NOTHING)
    store.upsert_identity(&u1.id, "google", "google-123").await.unwrap();
}

#[tokio::test]
async fn test_get_current_user() {
    let store = setup().await;
    let user = store.find_or_create_user("cur@test.com", None, None, &[]).await.unwrap();

    let current = store.get_current_user(&user.id).await.unwrap();
    assert_eq!(current.id, user.id);
    assert_eq!(current.email, "cur@test.com");
    assert_eq!(current.role, UserRole::Member);

    // Non-existent user
    assert!(store.get_current_user("nonexistent").await.is_err());
}
