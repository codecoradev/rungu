//! Tests for the board query module, through its interface. REST and MCP
//! only encode what these methods return, so paging, clamping and filter
//! rules are tested once, here.

use rungu_core::query::{BoardFilters, MAX_PER_PAGE, MAX_ROADMAP_BUCKET, PageRequest};
use rungu_core::{BoardQuery, OpError, Store, open_pool, run_migrations};
use rungu_proto::{PostCategory, PostStatus};

struct Fixture {
    store: Store,
    queries: BoardQuery,
    user_id: String,
    project_id: String,
}

impl Fixture {
    async fn new() -> Self {
        let pool = open_pool("sqlite::memory:").await.unwrap();
        run_migrations(&pool, "sqlite::memory:").await.unwrap();
        let store = Store::new_with_kind(pool, true);
        let project = store.create_project("Acme", "acme", "").await.unwrap();
        let user = store.find_or_create_user("u@x.test", None, None, &[]).await.unwrap();
        Self { queries: BoardQuery::new(store.clone()), store, user_id: user.id, project_id: project.id }
    }

    async fn post(&self, title: &str, category: PostCategory, status: PostStatus) -> String {
        let post = self.store.create_post(&self.project_id, title, "", category, &self.user_id).await.unwrap();
        if status != PostStatus::Open {
            self.store.update_post_status(&post.id, status).await.unwrap();
        }
        post.id
    }

    async fn vote(&self, post_id: &str, voters: usize) {
        for i in 0..voters {
            let voter = self.store.find_or_create_user(&format!("v{i}@x.test"), None, None, &[]).await.unwrap();
            self.store.toggle_vote(&voter.id, post_id).await.unwrap();
        }
    }
}

fn page(page: i64, per_page: i64) -> PageRequest {
    PageRequest { page: Some(page), per_page: Some(per_page) }
}

#[tokio::test]
async fn unknown_slug_is_not_found_everywhere() {
    let f = Fixture::new().await;
    let q = &f.queries;
    assert!(matches!(q.project("nope").await, Err(OpError::NotFound(_))));
    assert!(matches!(
        q.board("nope", BoardFilters::default(), PageRequest::default(), None).await,
        Err(OpError::NotFound(_))
    ));
    assert!(matches!(q.search("nope", "x", PageRequest::default()).await, Err(OpError::NotFound(_))));
    assert!(matches!(q.trending("nope", None).await, Err(OpError::NotFound(_))));
    assert!(matches!(q.changelog("nope", None, PageRequest::default()).await, Err(OpError::NotFound(_))));
    assert!(matches!(q.roadmap("nope", None).await, Err(OpError::NotFound(_))));
    assert!(matches!(q.similar_to_title("nope", "dark mode").await, Err(OpError::NotFound(_))));
    assert!(matches!(q.similar_to_post("missing").await, Err(OpError::NotFound(_))), "used to be a 500");
}

#[tokio::test]
async fn board_pages_with_defaults_and_clamps() {
    let f = Fixture::new().await;
    for i in 0..25 {
        f.post(&format!("Post {i}"), PostCategory::Feedback, PostStatus::Open).await;
    }

    let (project, first) =
        f.queries.board("acme", BoardFilters::default(), PageRequest::default(), None).await.unwrap();
    assert_eq!(project.slug, "acme");
    assert_eq!((first.posts.len(), first.total, first.page, first.per_page), (20, 25, 1, 20));
    assert_eq!(first.total_pages(), 2);

    let (_, second) = f.queries.board("acme", BoardFilters::default(), page(2, 20), None).await.unwrap();
    assert_eq!(second.posts.len(), 5);
    let first_ids: Vec<_> = first.posts.iter().map(|p| &p.post.id).collect();
    assert!(second.posts.iter().all(|p| !first_ids.contains(&&p.post.id)), "pages don't overlap");

    // Out-of-range values are clamped, never rejected.
    let (_, clamped) = f.queries.board("acme", BoardFilters::default(), page(0, 10_000), None).await.unwrap();
    assert_eq!((clamped.page, clamped.per_page), (1, MAX_PER_PAGE));
    let (_, tiny) = f.queries.board("acme", BoardFilters::default(), page(-5, -1), None).await.unwrap();
    assert_eq!((tiny.page, tiny.per_page, tiny.posts.len()), (1, 1, 1));
}

#[tokio::test]
async fn empty_board_still_has_one_page() {
    let f = Fixture::new().await;
    let (_, posts) = f.queries.board("acme", BoardFilters::default(), PageRequest::default(), None).await.unwrap();
    assert_eq!((posts.total, posts.total_pages()), (0, 1));
}

#[tokio::test]
async fn board_filters_parse_and_reject_unknown_values() {
    let f = Fixture::new().await;
    f.post("Crash on save", PostCategory::Bug, PostStatus::Open).await;
    f.post("Dark mode", PostCategory::Feature, PostStatus::Planned).await;

    let bugs = BoardFilters { category: Some("bug"), ..Default::default() };
    let (_, found) = f.queries.board("acme", bugs, PageRequest::default(), None).await.unwrap();
    assert_eq!(found.posts.iter().map(|p| p.post.title.as_str()).collect::<Vec<_>>(), ["Crash on save"]);

    let planned = BoardFilters { status: Some("planned"), ..Default::default() };
    let (_, found) = f.queries.board("acme", planned, PageRequest::default(), None).await.unwrap();
    assert_eq!(found.total, 1);

    // A typo must fail loudly instead of returning the unfiltered board.
    for filters in [
        BoardFilters { status: Some("opennn"), ..Default::default() },
        BoardFilters { category: Some("bugs"), ..Default::default() },
    ] {
        let result = f.queries.board("acme", filters, PageRequest::default(), None).await;
        assert!(matches!(result, Err(OpError::Invalid(_))), "{filters:?}");
    }

    // Blank search means no search.
    let blank = BoardFilters { q: Some("   "), ..Default::default() };
    assert_eq!(f.queries.board("acme", blank, PageRequest::default(), None).await.unwrap().1.total, 2);
}

#[tokio::test]
async fn search_requires_a_query() {
    let f = Fixture::new().await;
    f.post("Dark mode", PostCategory::Feature, PostStatus::Open).await;
    f.post("Export CSV", PostCategory::Feature, PostStatus::Open).await;

    assert!(matches!(f.queries.search("acme", "  ", PageRequest::default()).await, Err(OpError::Invalid(_))));
    let hits = f.queries.search("acme", "dark", PageRequest::default()).await.unwrap();
    assert_eq!(hits.posts.iter().map(|p| p.post.title.as_str()).collect::<Vec<_>>(), ["Dark mode"]);
}

#[tokio::test]
async fn roadmap_buckets_by_status_most_voted_first() {
    let f = Fixture::new().await;
    let low = f.post("Planned, few votes", PostCategory::Feature, PostStatus::Planned).await;
    let high = f.post("Planned, many votes", PostCategory::Feature, PostStatus::Planned).await;
    f.post("Building", PostCategory::Feature, PostStatus::InProgress).await;
    f.post("Shipped", PostCategory::Feature, PostStatus::Done).await;
    f.post("Just an idea", PostCategory::Feature, PostStatus::Open).await;
    f.vote(&low, 1).await;
    f.vote(&high, 3).await;

    let roadmap = f.queries.roadmap("acme", None).await.unwrap();
    let planned: Vec<_> = roadmap.planned.posts.iter().map(|p| p.post.id.as_str()).collect();
    assert_eq!(planned, [high.as_str(), low.as_str()]);
    assert_eq!((roadmap.planned.total, roadmap.in_progress.total, roadmap.done.total), (2, 1, 1));

    let one = f.queries.roadmap("acme", Some(1)).await.unwrap();
    assert_eq!((one.limit, one.planned.posts.len(), one.planned.total), (1, 1, 2));
    assert_eq!(f.queries.roadmap("acme", Some(999)).await.unwrap().limit, MAX_ROADMAP_BUCKET);
    assert_eq!(f.queries.roadmap("acme", Some(0)).await.unwrap().limit, 1);
}

#[tokio::test]
async fn changelog_lists_only_done_posts_and_validates_since() {
    let f = Fixture::new().await;
    f.post("Shipped", PostCategory::Feature, PostStatus::Done).await;
    f.post("Still open", PostCategory::Feature, PostStatus::Open).await;

    let log = f.queries.changelog("acme", None, PageRequest::default()).await.unwrap();
    assert_eq!(log.posts.iter().map(|p| p.post.title.as_str()).collect::<Vec<_>>(), ["Shipped"]);

    assert_eq!(
        f.queries.changelog("acme", Some("2999-01-01T00:00:00Z"), PageRequest::default()).await.unwrap().total,
        0
    );
    assert!(matches!(
        f.queries.changelog("acme", Some("yesterday"), PageRequest::default()).await,
        Err(OpError::Invalid(_))
    ));
}

#[tokio::test]
async fn similar_posts_exclude_the_post_itself() {
    let f = Fixture::new().await;
    let dark = f.post("Dark mode for the widget", PostCategory::Feature, PostStatus::Open).await;
    f.post("Dark theme toggle", PostCategory::Feature, PostStatus::Open).await;

    let similar = f.queries.similar_to_post(&dark).await.unwrap();
    assert!(similar.iter().all(|p| p.post.id != dark));
    assert_eq!(similar.iter().map(|p| p.post.title.as_str()).collect::<Vec<_>>(), ["Dark theme toggle"]);

    let drafts = f.queries.similar_to_title("acme", "Please add dark mode").await.unwrap();
    assert_eq!(drafts.len(), 2);
}
