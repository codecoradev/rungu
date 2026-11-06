//! Tests for the domain operations module, through its interface.
//!
//! Adapters at both seams are in-memory: a `RecordingSink` on the event bus
//! and a `MemStorage` behind the storage trait.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rungu_core::ops::{NewAttachment, NewPost, NewProject, NewWebhook, PostChanges, ProjectChanges, WebhookChanges};
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
async fn update_post_with_unchanged_values_publishes_nothing() {
    let f = Fixture::new().await;
    let id = f.post("Export CSV").await;

    let same = PostChanges { status: Some("open"), category: Some("feedback") };
    let updated = f.ops.update_post(&f.owner, &id, same).await.unwrap();
    assert_eq!(updated.post.status, PostStatus::Open);
    assert_eq!(f.events.events().len(), 1, "only the PostCreated from setup");
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

// ── Projects ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_project_is_admin_only_and_applies_slug_rules() {
    let f = Fixture::new().await;
    let named = |name| NewProject { name, ..Default::default() };

    assert!(matches!(f.ops.create_project(&f.owner, named("Mobile App")).await, Err(OpError::Forbidden(_))));

    let p = f.ops.create_project(&f.admin, named("  Mobile App! ")).await.unwrap();
    assert_eq!((p.name.as_str(), p.slug.as_str()), ("Mobile App!", "mobile-app"));

    // The rules every transport used to re-implement (MCP built slugs with
    // lower().replace(' ', "-") and skipped validation).
    let invalid = [
        NewProject { name: "  ", ..Default::default() },
        NewProject { name: &"x".repeat(101), ..Default::default() },
        NewProject { name: "Ok", slug: Some("   "), ..Default::default() },
        NewProject { name: "Ok", slug: Some("Has Spaces"), ..Default::default() },
        NewProject { name: "!!!", ..Default::default() }, // slugifies to ""
    ];
    for input in invalid {
        assert!(matches!(f.ops.create_project(&f.admin, input).await, Err(OpError::Invalid(_))), "{input:?}");
    }

    let dup = f.ops.create_project(&f.admin, NewProject { name: "Again", slug: Some("acme"), ..Default::default() });
    assert!(matches!(dup.await, Err(OpError::Conflict(_))), "duplicate slug is a conflict, not a 500");
}

#[tokio::test]
async fn update_project_validates_and_requires_admin() {
    let f = Fixture::new().await;
    let rename = ProjectChanges { name: Some("Acme Board"), description: None };

    assert!(matches!(f.ops.update_project(&f.owner, "acme", rename).await, Err(OpError::Forbidden(_))));
    assert!(matches!(f.ops.update_project(&f.admin, "nope", rename).await, Err(OpError::NotFound(_))));
    assert!(matches!(
        f.ops.update_project(&f.admin, "acme", ProjectChanges::default()).await,
        Err(OpError::Invalid(_))
    ));
    let blank = ProjectChanges { name: Some(" "), description: None };
    assert!(matches!(f.ops.update_project(&f.admin, "acme", blank).await, Err(OpError::Invalid(_))));

    assert_eq!(f.ops.update_project(&f.admin, "acme", rename).await.unwrap().name, "Acme Board");
}

#[tokio::test]
async fn delete_project_removes_attachment_files_of_every_post() {
    let f = Fixture::new().await;
    let mut paths = Vec::new();
    for title in ["One", "Two"] {
        let id = f.post(title).await;
        let path = f.storage.save(&format!("{title}.png"), vec![1]).await.unwrap();
        f.store().create_attachment(&id, "s.png", "image/png", 1, &path, &f.owner.id).await.unwrap();
        paths.push(path);
    }

    assert!(matches!(f.ops.delete_project(&f.owner, "acme").await, Err(OpError::Forbidden(_))));
    assert!(paths.iter().all(|p| f.storage.contains(p)));

    assert_eq!(f.ops.delete_project(&f.admin, "acme").await.unwrap().slug, "acme");
    assert!(f.store().get_project_by_slug("acme").await.unwrap().is_none());
    assert!(paths.iter().all(|p| !f.storage.contains(p)), "the cascade removes rows; ops must remove the files");
    assert!(matches!(f.ops.delete_project(&f.admin, "acme").await, Err(OpError::NotFound(_))));
}

// ── Webhooks ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_webhook_validates_the_url_and_generates_a_secret() {
    let f = Fixture::new().await;
    let hook = |url| NewWebhook { url, ..Default::default() };

    assert!(matches!(
        f.ops.create_webhook(&f.owner, "acme", hook("https://example.com/h")).await,
        Err(OpError::Forbidden(_))
    ));
    assert!(matches!(
        f.ops.create_webhook(&f.admin, "nope", hook("https://example.com/h")).await,
        Err(OpError::NotFound(_))
    ));
    let long = format!("https://example.com/{}", "a".repeat(2048));
    for url in ["", "http://example.com/h", "ftp://example.com", "https://169.254.169.254/x", long.as_str()] {
        assert!(matches!(f.ops.create_webhook(&f.admin, "acme", hook(url)).await, Err(OpError::Invalid(_))), "{url}");
    }

    let created = f.ops.create_webhook(&f.admin, "acme", hook(" https://example.com/h ")).await.unwrap();
    assert_eq!(created.webhook.url, "https://example.com/h");
    assert!(!created.secret.is_empty(), "a secret is generated when none is given");
    // A supplied secret is used exactly as given; receivers sign with it.
    let given = NewWebhook { url: "https://example.com/g", events: Some("post.created"), secret: Some(" s3cret") };
    assert_eq!(f.ops.create_webhook(&f.admin, "acme", given).await.unwrap().secret, " s3cret");
    let blank_events = NewWebhook { url: "https://example.com/b", events: Some("  "), secret: None };
    assert!(matches!(f.ops.create_webhook(&f.admin, "acme", blank_events).await, Err(OpError::Invalid(_))));
}

#[tokio::test]
async fn create_project_ignores_a_stale_slug_cache() {
    let f = Fixture::new().await;
    // Warm the slug cache, then delete the row the way another process (e.g.
    // `rungu mcp`) would: straight in the database, without this cache seeing it.
    assert!(f.store().get_project_by_slug("acme").await.unwrap().is_some());
    sqlx::query("DELETE FROM projects WHERE slug = 'acme'").execute(f.store().pool()).await.unwrap();

    let again = NewProject { name: "Acme", slug: Some("acme"), ..Default::default() };
    let p = f.ops.create_project(&f.admin, again).await.expect("a stale cache entry must not cause a false conflict");
    assert_eq!(p.slug, "acme");
}

#[tokio::test]
async fn update_project_checks_input_before_existence() {
    let f = Fixture::new().await;
    // Admin-only, so the old REST order is kept: an empty body is a 400 even
    // for an unknown slug.
    assert!(matches!(
        f.ops.update_project(&f.admin, "nope", ProjectChanges::default()).await,
        Err(OpError::Invalid(_))
    ));
}

#[tokio::test]
async fn webhooks_of_another_project_read_as_not_found() {
    let f = Fixture::new().await;
    f.ops.create_project(&f.admin, NewProject { name: "Other", ..Default::default() }).await.unwrap();
    let hook = NewWebhook { url: "https://example.com/h", ..Default::default() };
    let id = f.ops.create_webhook(&f.admin, "acme", hook).await.unwrap().webhook.id;

    let pause = WebhookChanges { is_active: Some(false), ..Default::default() };
    assert!(matches!(f.ops.update_webhook(&f.admin, "other", &id, pause).await, Err(OpError::NotFound(_))));
    assert!(matches!(f.ops.delete_webhook(&f.admin, &id, Some("other")).await, Err(OpError::NotFound(_))));

    let bad_url = WebhookChanges { url: Some("http://example.com"), ..Default::default() };
    assert!(matches!(f.ops.update_webhook(&f.admin, "acme", &id, bad_url).await, Err(OpError::Invalid(_))));
    assert!(!f.ops.update_webhook(&f.admin, "acme", &id, pause).await.unwrap().is_active);
}

#[tokio::test]
async fn delete_webhook_requires_an_existing_webhook() {
    let f = Fixture::new().await;
    let hook = NewWebhook { url: "https://example.com/h", ..Default::default() };
    let id = f.ops.create_webhook(&f.admin, "acme", hook).await.unwrap().webhook.id;

    // MCP addresses webhooks by id only: an unknown id must be an error, not
    // a silent success.
    assert!(matches!(f.ops.delete_webhook(&f.admin, "missing", None).await, Err(OpError::NotFound(_))));
    assert!(matches!(f.ops.delete_webhook(&f.owner, &id, None).await, Err(OpError::Forbidden(_))));
    f.ops.delete_webhook(&f.admin, &id, None).await.unwrap();
    assert!(f.store().get_webhook(&id).await.unwrap().is_none());
}

// ── Attachment upload ───────────────────────────────────────────────────

/// Smallest byte string `verify_image` accepts as a PNG.
fn png() -> Vec<u8> {
    let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    data.extend_from_slice(&[0; 16]);
    data
}

#[tokio::test]
async fn upload_attachment_checks_owner_and_image_then_stores_file_and_row() {
    let f = Fixture::new().await;
    let id = f.post("Needs a screenshot").await;
    let upload = |data| NewAttachment { filename: "shot.png", content_type: "image/png", data };

    assert!(matches!(f.ops.check_can_upload(&f.stranger, &id).await, Err(OpError::Forbidden(_))));
    assert!(matches!(f.ops.upload_attachment(&f.stranger, &id, upload(png())).await, Err(OpError::Forbidden(_))));
    assert!(matches!(f.ops.upload_attachment(&f.owner, "missing", upload(png())).await, Err(OpError::NotFound(_))));
    assert!(matches!(f.ops.upload_attachment(&f.owner, &id, upload(vec![])).await, Err(OpError::Invalid(_))));
    assert!(matches!(
        f.ops.upload_attachment(&f.owner, &id, upload(b"not an image".to_vec())).await,
        Err(OpError::Invalid(_))
    ));
    let spoofed = NewAttachment { filename: "x.jpg", content_type: "image/jpeg", data: png() };
    assert!(matches!(f.ops.upload_attachment(&f.owner, &id, spoofed).await, Err(OpError::Invalid(_))));

    let attachment = f.ops.upload_attachment(&f.owner, &id, upload(png())).await.unwrap();
    assert_eq!((attachment.post_id.as_str(), attachment.mime.as_str()), (id.as_str(), "image/png"));
    let (_, path) = f.store().get_attachment(&attachment.id).await.unwrap().unwrap();
    assert!(f.storage.contains(&path));

    // Admins may attach to anyone's post.
    f.ops.upload_attachment(&f.admin, &id, upload(png())).await.unwrap();
    assert_eq!(f.store().list_attachments(&id).await.unwrap().len(), 2);
}

/// The flags clients use to decide which controls to show come from the same
/// rule that refuses the call: whatever `post_permissions` says, the operation
/// agrees.
#[tokio::test]
async fn reported_permissions_match_what_is_enforced() {
    let f = Fixture::new().await;
    let author_id = f.owner.id.clone();

    for actor in [&f.owner, &f.stranger, &f.admin] {
        let flags = actor.post_permissions(&author_id);

        let id = f.post("Flagged").await; // written by the owner
        let changed =
            f.ops.update_post(actor, &id, PostChanges { status: Some("planned"), ..Default::default() }).await;
        assert_eq!(changed.is_ok(), flags.can_edit, "can_edit vs update_post for {:?}", actor.role);
        if let Err(e) = changed {
            assert!(matches!(e, OpError::Forbidden(_)));
        }

        let pinned = f.ops.set_official_response(actor, &id, None).await;
        assert_eq!(pinned.is_ok(), flags.can_moderate, "can_moderate vs set_official_response for {:?}", actor.role);

        let doomed = f.post("To delete").await;
        let deleted = f.ops.delete_post(actor, &doomed).await;
        assert_eq!(deleted.is_ok(), flags.can_edit, "can_edit vs delete_post for {:?}", actor.role);
    }

    // Nobody but the author and admins edits; only admins moderate.
    assert!(!f.stranger.post_permissions(&author_id).can_edit);
    assert!(f.owner.post_permissions(&author_id).can_edit);
    assert!(!f.owner.post_permissions(&author_id).can_moderate);
    assert!(f.admin.post_permissions(&author_id).can_moderate);
}
