// Board state — everything the board page does with data, apart from drawing it.
//
// The board page used to carry this inline: the stale-response guard (#173),
// refetch dimming (#148), sort ↔ URL sync (#203), paging with de-duplication,
// best-effort counts and the search debounce. It lives here so it can be
// tested with a fake `api` (the seam): the page only renders and forwards events.

import { ApiError } from '$lib/api/client';
import type { PaginatedResponse, PostCategory, PostDetail, PostStatus, Project, VoteResponse } from '$lib/api/types';

/** Posts per request, for the first page and for every "show more". */
export const PAGE_SIZE = 50;
/** Typing in the search box waits this long before asking the server. */
export const SEARCH_DEBOUNCE_MS = 300;

export const SORT_OPTIONS = [
    { value: 'newest', label: 'Newest' },
    { value: 'trending', label: 'Trending' },
    { value: 'most_votes', label: 'Most Voted' },
    { value: 'recently_updated', label: 'Recently Updated' },
] as const;
export type Sort = (typeof SORT_OPTIONS)[number]['value'];
export const DEFAULT_SORT: Sort = 'newest';

export interface BoardCounts {
    by_status: Record<string, number>;
    by_category: Record<string, number>;
}

/** The part of the REST client the board uses. */
export interface BoardApi {
    getProject(slug: string): Promise<Project>;
    listPosts(
        slug: string,
        params: { sort?: string; status?: PostStatus; category?: PostCategory; q?: string; page?: number; per_page?: number },
    ): Promise<PaginatedResponse<PostDetail>>;
    getProjectCounts(slug: string): Promise<BoardCounts>;
    toggleVote(postId: string): Promise<VoteResponse>;
    createPost(slug: string, body: { title: string; description?: string; category?: PostCategory }): Promise<unknown>;
}

// ── Sort ↔ URL (#203) ───────────────────────────────────────────────

/** The sort named by `?sort=`; anything unknown falls back to the default
 *  (the server clamps unknown values to newest too). */
export function sortFromSearch(search: string): Sort {
    const value = new URLSearchParams(search).get('sort');
    return SORT_OPTIONS.find((o) => o.value === value)?.value ?? DEFAULT_SORT;
}

/** `href` with `?sort=` set for `sort` (and removed for the default). */
export function hrefWithSort(href: string, sort: Sort): string {
    const url = new URL(href);
    if (sort === DEFAULT_SORT) url.searchParams.delete('sort');
    else url.searchParams.set('sort', sort);
    return url.href;
}

// ── The board ───────────────────────────────────────────────────────

export interface BoardOptions {
    /** Called when "show more" fails (the board itself stays as it was). */
    onLoadMoreError?: () => void;
    initialSort?: Sort;
}

export function createBoardState(api: BoardApi, options: BoardOptions = {}) {
    let project = $state<Project | null>(null);
    let posts = $state<PostDetail[]>([]);
    let total = $state(0);
    let page = $state(1);
    // True once a "show more" brought nothing new (every post on it was already
    // shown), so the button goes away instead of asking for the same end forever.
    let exhausted = $state(false);
    let loading = $state(true);
    // True while re-fetching with posts already on screen (filter/search
    // refill): gates the dimming overlay so the first load still shows skeletons. #148
    let refetching = $state(false);
    let error = $state('');
    let counts = $state<BoardCounts | null>(null);
    let sort = $state<Sort>(options.initialSort ?? DEFAULT_SORT);
    let statusFilter = $state<PostStatus | ''>('');
    let categoryFilter = $state<PostCategory | ''>('');
    let searchQuery = $state('');
    let focusedIndex = $state(-1);

    // Monotonic guard against stale responses (#173): every load bumps it, and
    // only the latest generation may commit what it fetched.
    let generation = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;

    function query() {
        return {
            sort,
            status: statusFilter || undefined,
            category: categoryFilter || undefined,
            q: searchQuery || undefined,
            per_page: PAGE_SIZE,
        };
    }

    /** (Re)load the first page for `slug` with the current filters. */
    async function load(slug: string): Promise<void> {
        refetching = posts.length > 0;
        loading = posts.length === 0;
        error = '';
        const mine = ++generation;
        try {
            const projectData = await api.getProject(slug);
            const result = await api.listPosts(slug, { ...query(), page: 1 });
            if (mine !== generation) return; // a newer load superseded this one
            project = projectData;
            posts = result.data;
            total = result.pagination.total;
            page = 1;
            exhausted = false;
            focusedIndex = -1;
            // Counts are best-effort: their absence (or lateness) never breaks the board.
            api.getProjectCounts(slug)
                .then((c) => {
                    if (mine === generation) counts = c;
                })
                .catch(() => {});
        } catch (e) {
            if (mine !== generation) return;
            error = e instanceof ApiError && e.status === 404 ? 'Project not found' : 'Failed to load board';
        } finally {
            if (mine === generation) {
                loading = false;
                refetching = false;
            }
        }
    }

    /** Append the next page, skipping posts already shown. */
    async function loadMore(slug: string): Promise<void> {
        // Not while the board is being (re)loaded: the answer would land on
        // top of a page-1 list and skip the pages in between.
        if (loading || refetching || posts.length >= total || exhausted) return;
        const mine = generation;
        const next = page + 1;
        try {
            const result = await api.listPosts(slug, { ...query(), page: next });
            if (mine !== generation) return; // filters changed while this was in flight
            const seen = new Set(posts.map((p) => p.id));
            const fresh = result.data.filter((p) => !seen.has(p.id));
            posts = [...posts, ...fresh];
            total = result.pagination.total;
            page = next;
            if (fresh.length === 0) exhausted = true;
        } catch {
            if (mine === generation) options.onLoadMoreError?.();
        }
    }

    /** Reload after a filter, sort or slug change; typing waits for a pause. */
    function queueReload(slug: string): () => void {
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => void load(slug), searchQuery ? SEARCH_DEBOUNCE_MS : 0);
        return cancel;
    }

    function cancel() {
        if (timer) clearTimeout(timer);
        timer = null;
    }

    async function toggleVote(post: PostDetail): Promise<void> {
        try {
            const result = await api.toggleVote(post.id);
            post.user_voted = result.voted;
            post.vote_count = result.vote_count;
        } catch {
            // A failed vote isn't worth a banner here.
        }
    }

    async function createPost(
        slug: string,
        data: { title: string; description: string; category: PostCategory },
    ): Promise<void> {
        await api.createPost(slug, data);
        // Refresh in the background: the caller can close its form now instead
        // of keeping it open for the whole reload.
        void load(slug);
    }

    return {
        get project() {
            return project;
        },
        get posts() {
            return posts;
        },
        get total() {
            return total;
        },
        get loading() {
            return loading;
        },
        get refetching() {
            return refetching;
        },
        get error() {
            return error;
        },
        get counts() {
            return counts;
        },
        get sort() {
            return sort;
        },
        set sort(v: Sort) {
            sort = v;
        },
        get statusFilter() {
            return statusFilter;
        },
        set statusFilter(v: PostStatus | '') {
            statusFilter = v;
        },
        get categoryFilter() {
            return categoryFilter;
        },
        set categoryFilter(v: PostCategory | '') {
            categoryFilter = v;
        },
        get searchQuery() {
            return searchQuery;
        },
        set searchQuery(v: string) {
            searchQuery = v;
        },
        get focusedIndex() {
            return focusedIndex;
        },
        /** Whether "show more" has anything left to fetch. */
        get hasMore() {
            return posts.length < total && !exhausted;
        },
        get hasFilters() {
            return Boolean(searchQuery || statusFilter || categoryFilter);
        },
        toggleStatus(v: PostStatus) {
            statusFilter = statusFilter === v ? '' : v;
        },
        toggleCategory(v: PostCategory) {
            categoryFilter = categoryFilter === v ? '' : v;
        },
        clearFilters() {
            searchQuery = '';
            statusFilter = '';
            categoryFilter = '';
        },
        /** j / k: move the keyboard focus through the posts on screen. */
        focusNext() {
            if (posts.length === 0) return false;
            focusedIndex = Math.min(focusedIndex + 1, posts.length - 1);
            return true;
        },
        focusPrev() {
            if (posts.length === 0) return false;
            focusedIndex = Math.max(focusedIndex - 1, 0);
            return true;
        },
        get focusedPost(): PostDetail | undefined {
            return posts[focusedIndex];
        },
        load,
        loadMore,
        queueReload,
        cancel,
        toggleVote,
        createPost,
    };
}

export type BoardState = ReturnType<typeof createBoardState>;
