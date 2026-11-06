//! Board query module — the read side of a board, shared by every transport.
//!
//! REST handlers and MCP tools call one method here and encode the result.
//! The module owns what each caller used to repeat: slug → project
//! resolution (unknown slug = [`OpError::NotFound`]), paging defaults and
//! clamps, filter parsing, and the fixed sorts of the roadmap, changelog and
//! trending views. SQL stays in [`Store`]. Reads are public, so there is no
//! authorization step; the optional viewer only fills `user_voted`.

use chrono::{DateTime, Utc};
use rungu_proto::{ListPostsParams, PostDetail, PostSort, PostStatus, Project};

use crate::Store;
use crate::ops::{OpError, OpResult, parse_category, parse_sort, parse_status};

/// Posts per page when the caller doesn't say.
pub const DEFAULT_PER_PAGE: i64 = 20;
/// Largest page a caller may ask for.
pub const MAX_PER_PAGE: i64 = 100;
/// Highest page number accepted (keeps the offset arithmetic far from overflow).
pub const MAX_PAGE: i64 = 10_000_000;
/// Posts per roadmap bucket when the caller doesn't say.
pub const DEFAULT_ROADMAP_BUCKET: i64 = 10;
/// Largest roadmap bucket: one call never returns more than 3 × 50 posts.
pub const MAX_ROADMAP_BUCKET: i64 = 50;
/// Posts in the trending view when the caller doesn't say.
pub const DEFAULT_TRENDING: i64 = 10;
/// Most similar posts returned for a post or a draft title.
pub const SIMILAR_LIMIT: i64 = 3;
/// Longest draft title considered for the duplicate check (posts cap titles at 200).
const MAX_DRAFT_TITLE_CHARS: usize = 200;

/// Which page of results to return. `None` fields take the defaults; values
/// out of range are clamped, never rejected.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageRequest {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Board filters as the transports receive them: raw strings, parsed here so
/// REST and MCP share one parser and one set of error messages.
#[derive(Debug, Clone, Copy, Default)]
pub struct BoardFilters<'a> {
    /// Unknown or absent = newest first.
    pub sort: Option<&'a str>,
    pub status: Option<&'a str>,
    pub category: Option<&'a str>,
    /// Full-text search; blank = no search.
    pub q: Option<&'a str>,
}

/// One page of posts plus what a caller needs to page through the rest.
#[derive(Debug, Clone)]
pub struct PostPage {
    pub posts: Vec<PostDetail>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

impl PostPage {
    /// At least 1, so an empty board still reports "page 1 of 1".
    pub fn total_pages(&self) -> i64 {
        ((self.total + self.per_page - 1) / self.per_page).max(1)
    }
}

/// One roadmap column: its top posts by votes and how many it holds in total.
#[derive(Debug, Clone)]
pub struct RoadmapBucket {
    pub posts: Vec<PostDetail>,
    pub total: i64,
}

/// The committed-work view: planned, in progress and done, most voted first.
#[derive(Debug, Clone)]
pub struct Roadmap {
    pub planned: RoadmapBucket,
    pub in_progress: RoadmapBucket,
    pub done: RoadmapBucket,
    /// The per-bucket limit actually applied.
    pub limit: i64,
}

/// Read access to boards. Cheap to clone; clones share the store and its
/// project cache.
#[derive(Clone)]
pub struct BoardQuery {
    store: Store,
}

impl BoardQuery {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// The project behind a slug.
    pub async fn project(&self, slug: &str) -> OpResult<Project> {
        self.store.get_project_by_slug(slug).await?.ok_or_else(|| OpError::NotFound("Project not found".into()))
    }

    /// A page of a board's posts, filtered and sorted. Returns the project
    /// too, so callers can record a board view without a second lookup.
    pub async fn board(
        &self,
        slug: &str,
        filters: BoardFilters<'_>,
        page: PageRequest,
        viewer_id: Option<&str>,
    ) -> OpResult<(Project, PostPage)> {
        let project = self.project(slug).await?;
        let status = filters
            .status
            .map(|s| {
                parse_status(s).ok_or_else(|| {
                    OpError::Invalid(format!(
                        "Invalid status filter '{s}'; expected open, planned, in_progress, done or declined"
                    ))
                })
            })
            .transpose()?;
        let category = filters
            .category
            .map(|c| {
                parse_category(c).ok_or_else(|| {
                    OpError::Invalid(format!(
                        "Invalid category filter '{c}'; expected feedback, bug, feature or question"
                    ))
                })
            })
            .transpose()?;
        let q = filters.q.map(str::trim).filter(|q| !q.is_empty());

        let posts = self
            .page(&project, parse_sort(filters.sort), status, category, q, None, viewer_id, page, DEFAULT_PER_PAGE)
            .await?;
        Ok((project, posts))
    }

    /// Full-text search, newest first. The query must not be blank.
    pub async fn search(&self, slug: &str, q: &str, page: PageRequest) -> OpResult<PostPage> {
        let q = q.trim();
        if q.is_empty() {
            return Err(OpError::Invalid("Search query is required".into()));
        }
        let project = self.project(slug).await?;
        self.page(&project, PostSort::Newest, None, None, Some(q), None, None, page, DEFAULT_PER_PAGE).await
    }

    /// Posts gaining votes fastest (7-day velocity, tie-broken by totals).
    pub async fn trending(&self, slug: &str, limit: Option<i64>) -> OpResult<PostPage> {
        let project = self.project(slug).await?;
        let page = PageRequest { page: None, per_page: limit };
        self.page(&project, PostSort::Trending, None, None, None, None, None, page, DEFAULT_TRENDING).await
    }

    /// Shipped posts (status done), most recently updated first. `since` is
    /// an RFC 3339 timestamp; a malformed one is rejected rather than ignored,
    /// so a client bug never silently returns the full history.
    pub async fn changelog(&self, slug: &str, since: Option<&str>, page: PageRequest) -> OpResult<PostPage> {
        let project = self.project(slug).await?;
        let since = since
            .map(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|_| OpError::Invalid("Invalid `since` timestamp; expected RFC3339".into()))
            })
            .transpose()?;
        let status = Some(PostStatus::Done);
        self.page(&project, PostSort::RecentlyUpdated, status, None, None, since, None, page, DEFAULT_PER_PAGE).await
    }

    /// The roadmap: planned, in progress and done, each most voted first.
    pub async fn roadmap(&self, slug: &str, per_bucket: Option<i64>) -> OpResult<Roadmap> {
        let project = self.project(slug).await?;
        let limit = per_bucket.unwrap_or(DEFAULT_ROADMAP_BUCKET).clamp(1, MAX_ROADMAP_BUCKET);
        Ok(Roadmap {
            planned: self.roadmap_bucket(&project, PostStatus::Planned, limit).await?,
            in_progress: self.roadmap_bucket(&project, PostStatus::InProgress, limit).await?,
            done: self.roadmap_bucket(&project, PostStatus::Done, limit).await?,
            limit,
        })
    }

    async fn roadmap_bucket(&self, project: &Project, status: PostStatus, limit: i64) -> OpResult<RoadmapBucket> {
        let page = PageRequest { page: Some(1), per_page: Some(limit) };
        let posts = self.page(project, PostSort::MostVotes, Some(status), None, None, None, None, page, limit).await?;
        Ok(RoadmapBucket { posts: posts.posts, total: posts.total })
    }

    /// Posts similar to an existing post (same board, the post itself excluded).
    pub async fn similar_to_post(&self, post_id: &str) -> OpResult<Vec<PostDetail>> {
        let post =
            self.store.get_post(post_id, None).await?.ok_or_else(|| OpError::NotFound("Post not found".into()))?;
        Ok(self
            .store
            .find_similar_by_title(&post.post.project_id, &post.post.title, Some(post_id), SIMILAR_LIMIT)
            .await?)
    }

    /// Existing posts that look like a draft title (the as-you-type duplicate check).
    pub async fn similar_to_title(&self, slug: &str, title: &str) -> OpResult<Vec<PostDetail>> {
        let project = self.project(slug).await?;
        let title: String = title.chars().take(MAX_DRAFT_TITLE_CHARS).collect();
        Ok(self.store.find_similar_by_title(&project.id, &title, None, SIMILAR_LIMIT).await?)
    }

    /// The one place a page request becomes a store query.
    #[allow(clippy::too_many_arguments)]
    async fn page(
        &self,
        project: &Project,
        sort: PostSort,
        status: Option<PostStatus>,
        category: Option<rungu_proto::PostCategory>,
        query: Option<&str>,
        since: Option<DateTime<Utc>>,
        user_id: Option<&str>,
        request: PageRequest,
        default_per_page: i64,
    ) -> OpResult<PostPage> {
        let page = request.page.unwrap_or(1).clamp(1, MAX_PAGE);
        let per_page = request.per_page.unwrap_or(default_per_page).clamp(1, MAX_PER_PAGE);
        let params = ListPostsParams {
            project_id: &project.id,
            sort,
            status,
            category,
            query,
            since,
            user_id,
            offset: (page - 1) * per_page,
            limit: per_page,
        };
        let (posts, total) = self.store.list_posts(params).await?;
        Ok(PostPage { posts, total, page, per_page })
    }
}
