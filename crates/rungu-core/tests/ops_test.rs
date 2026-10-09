//! Tests for the domain operations module, through its interface.
//!
//! Adapters at both seams are in-memory: a `RecordingSink` on the event bus
//! and a `MemStorage` behind the storage trait.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rungu_core::ops::{NewPost, PostChanges};
use rungu_core::{
    Actor, DomainEvent, EventBus, OpError, Operations, RecordingSink, Storage, Store, open_pool, run_migrations,
};
use rungu_proto::{PostCategory, PostStatus, UserRole};

#[derive(Default)]
struct MemStorage {
    files: Mutex<HashSet<String>>,
}

impl MemStorage {
    fn contains(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains(path)
    }
}

#[async_trait::async_trait]
impl Storage for MemStorage {
    async fn save(&self, key: &str, _data: Vec<u8>) -> anyhow::Result<String> {
        self.files.lock().unwrap().insert(key.to_string());
        Ok(key.to_string())
    }
    async fn load(&self, _path: &str) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.files.lock().unwrap().remove(path);
        Ok(())
    }
}

struct Fixture {
    ops: Operations,
    events: Arc<RecordingSink>,
    storage: Arc<MemStorage>,
    owner: Actor,
    stranger: Actor,
    admin: Actor,
}

impl Fixture {
    async fn new() -> Self {
        let pool = open_pool("sqlite::memory:").await.unwrap();
        run_migrations(&pool, "sqlite::memory:").await.unwrap();
        let store = Store::new_with_kind(pool, true);
        store.create_project("Acme", "acme", "").await.unwrap();

        let mut actors = Vec::new();
        for (email, role) in [
            ("owner@x.test", UserRole::Member),
            ("stranger@x.test", UserRole::Member),
            ("admin@x.test", UserRole::Admin),
        ] {
            let user = store.find_or_create_user(email, None, None, &[]).await.unwrap();
            actors.push(Actor { id: user.id, role });
        }
        let [owner, stranger, admin] = actors.try_into().unwrap();

        let events = Arc::new(RecordingSink::default());
        let storage = Arc::new(MemStorage::default());
        let ops = Operations::new(store, storage.clone(), EventBus::new(vec![events.clone()]));
        Self { ops, events, storage, owner, stranger, admin }
    }

    fn store(&self) -> &Store {
        self.ops.store()
    }

    async fn post(&self, title: &str) -> String {
        self.ops.create_post(&self.owner, "acme", NewPost { title, ..Default::default() }).await.unwrap().id
    }
}

// ── Posts ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_post_trims_validates_and_publishes() {
    let f = Fixture::new().await;

    let post = f
        .ops
        .create_post(
            &f.owner,
            "acme",
            NewPost { title: "  Dark mode  ", category: Some("feature"), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(post.title, "Dark mode");
    assert_eq!(post.category, PostCategory::Feature);
    assert!(matches!(&f.events.events()[..], [DomainEvent::PostCreated { post: p, actor_id }]
        if p.id == post.id && *actor_id == f.owner.id));

    for (title, category, msg) in [
        ("   ", None, "Title is required"),
        (&*"x".repeat(201), None, "Title must be 200 characters or less"),
        ("Fine", Some("nope"), "Invalid category"),
    ] {
        let err = f.ops.create_post(&f.owner, "acme", NewPost { title, category, ..Default::default() }).await;
        assert!(matches!(err, Err(OpError::Invalid(m)) if m == msg));
    }
    let missing = f.ops.create_post(&f.owner, "nope", NewPost { title: "x", ..Default::default() }).await;
    assert!(matches!(missing, Err(OpError::NotFound(_))));
    assert_eq!(f.events.events().len(), 1, "refused operations publish nothing");
}

#[tokio::test]
async fn update_post_checks_ownership_before_validating_input() {
    let f = Fixture::new().await;
    let id = f.post("Export CSV").await;

    // A stranger sending an empty body must get 403, not 400 (#162).
    let err = f.ops.update_post(&f.stranger, &id, PostChanges::default()).await;
    assert!(matches!(err, Err(OpError::Forbidden(_))));

    let err = f.ops.update_post(&f.owner, &id, PostChanges::default()).await;
    assert!(matches!(err, Err(OpError::Invalid(m)) if m == "No fields to update"));
}

#[tokio::test]
async fn update_post_is_all_or_nothing() {
    let f = Fixture::new().await;
    let id = f.post("Export CSV").await;

    let err = f.ops.update_post(&f.owner, &id, PostChanges { status: Some("done"), category: Some("bogus") }).await;
    assert!(matches!(err, Err(OpError::Invalid(m)) if m == "Invalid category"));
    let post = f.store().get_post(&id, None).await.unwrap().unwrap().post;
    assert_eq!(post.status, PostStatus::Open, "status must not change when the category is invalid");
}

#[tokio::test]
async fn update_post_publishes_one_event_per_changed_field() {
    let f = Fixture::new().await;
    let id = f.post("Export CSV").await;

    let updated =
        f.ops.update_post(&f.admin, &id, PostChanges { status: Some("planned"), category: Some("bug") }).await.unwrap();
    assert_eq!(updated.post.status, PostStatus::Planned);
    assert_eq!(updated.post.category, PostCategory::Bug);

    let events = f.events.events();
    assert!(matches!(&events[1], DomainEvent::PostStatusChanged {
        old_status: PostStatus::Open, new_status: PostStatus::Planned, actor_id, ..
    } if *actor_id == f.admin.id));
    assert!(matches!(
        &events[2],
        DomainEvent::PostCategoryChanged { old_category: PostCategory::Feedback, new_category: PostCategory::Bug, .. }
    ));
}

#[tokio::test]
async fn delete_post_removes_attachment_files() {
    let f = Fixture::new().await;
    let id = f.post("Has screenshot").await;
    let path = f.storage.save("shot.png", vec![1]).await.unwrap();
    f.store().create_attachment(&id, "shot.png", "image/png", 1, &path, &f.owner.id).await.unwrap();

    assert!(matches!(f.ops.delete_post(&f.stranger, &id).await, Err(OpError::Forbidden(_))));
    assert!(f.storage.contains(&path));

    f.ops.delete_post(&f.owner, &id).await.unwrap();
    assert!(f.store().get_post(&id, None).await.unwrap().is_none());
    assert!(!f.storage.contains(&path), "the cascade removes the row; ops must remove the file");
    assert!(matches!(f.events.events().last(), Some(DomainEvent::PostDeleted { .. })));
}

// ── Votes ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn toggle_vote_round_trips_and_requires_an_existing_post() {
    let f = Fixture::new().await;
    let id = f.post("Slack integration").await;

    let on = f.ops.toggle_vote(&f.stranger, &id).await.unwrap();
    assert!(on.voted);
    assert_eq!(on.vote_count, 1);
    let off = f.ops.toggle_vote(&f.stranger, &id).await.unwrap();
    assert!(!off.voted);
    assert_eq!(off.vote_count, 0);

    assert!(matches!(f.ops.toggle_vote(&f.stranger, "missing").await, Err(OpError::NotFound(_))));
    let votes = f.events.events().iter().filter(|e| matches!(e, DomainEvent::VoteToggled { .. })).count();
    assert_eq!(votes, 2);
}

// ── Comments ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_comment_validates_content_and_thread() {
    let f = Fixture::new().await;
    let a = f.post("Post A").await;
    let b = f.post("Post B").await;

    let empty = f.ops.create_comment(&f.stranger, &a, "   ", None).await;
    assert!(matches!(empty, Err(OpError::Invalid(m)) if m == "Comment content is required"));
    let long = f.ops.create_comment(&f.stranger, &a, &"x".repeat(4001), None).await;
    assert!(matches!(long, Err(OpError::Invalid(_))));

    let on_b = f.ops.create_comment(&f.stranger, &b, "on B", None).await.unwrap();
    let cross = f.ops.create_comment(&f.stranger, &a, "reply", Some(&on_b.comment.id)).await;
    assert!(matches!(cross, Err(OpError::Invalid(m)) if m == "Parent comment does not belong to this post"));

    let reply = f.ops.create_comment(&f.owner, &b, "  thanks  ", Some(&on_b.comment.id)).await.unwrap();
    assert_eq!(reply.comment.content, "thanks");
    assert!(matches!(f.events.events().last(), Some(DomainEvent::CommentCreated { post_title, actor_id, .. })
        if post_title == "Post B" && *actor_id == f.owner.id));
}

#[tokio::test]
async fn delete_comment_is_author_or_admin() {
    let f = Fixture::new().await;
    let id = f.post("Post").await;
    let c = f.ops.create_comment(&f.owner, &id, "mine", None).await.unwrap();

    assert!(matches!(f.ops.delete_comment(&f.stranger, &c.comment.id).await, Err(OpError::Forbidden(_))));
    f.ops.delete_comment(&f.admin, &c.comment.id).await.unwrap();
    assert!(matches!(f.ops.delete_comment(&f.admin, &c.comment.id).await, Err(OpError::NotFound(_))));
}

#[tokio::test]
async fn official_response_is_admin_only_and_scoped_to_the_post() {
    let f = Fixture::new().await;
    let a = f.post("Post A").await;
    let b = f.post("Post B").await;
    let on_a = f.ops.create_comment(&f.owner, &a, "answer", None).await.unwrap();
    let on_b = f.ops.create_comment(&f.owner, &b, "elsewhere", None).await.unwrap();

    // Non-admins are refused before existence is revealed.
    let err = f.ops.set_official_response(&f.owner, "missing", Some(&on_a.comment.id)).await;
    assert!(matches!(err, Err(OpError::Forbidden(_))));

    let err = f.ops.set_official_response(&f.admin, &a, Some(&on_b.comment.id)).await;
    assert!(matches!(err, Err(OpError::Invalid(m)) if m == "Comment does not belong to this post"));

    let set = f.ops.set_official_response(&f.admin, &a, Some(&on_a.comment.id)).await.unwrap();
    assert_eq!(set.previous_comment_id, None);
    assert_eq!(set.response.unwrap().comment.id, on_a.comment.id);

    let cleared = f.ops.set_official_response(&f.admin, &a, None).await.unwrap();
    assert_eq!(cleared.previous_comment_id.as_deref(), Some(on_a.comment.id.as_str()));
    assert!(cleared.response.is_none());
}

// ── Attachments ─────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_attachment_removes_file_and_row() {
    let f = Fixture::new().await;
    let id = f.post("Post").await;
    let path = f.storage.save("a.png", vec![1]).await.unwrap();
    let att = f.store().create_attachment(&id, "a.png", "image/png", 1, &path, &f.owner.id).await.unwrap();

    assert!(matches!(f.ops.delete_attachment(&f.stranger, &att.id).await, Err(OpError::Forbidden(_))));
    f.ops.delete_attachment(&f.owner, &att.id).await.unwrap();
    assert!(!f.storage.contains(&path));
    assert!(f.store().get_attachment(&att.id).await.unwrap().is_none());
}
