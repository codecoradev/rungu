import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/client';
import type { PostDetail, Project } from '$lib/api/types';
import {
    PAGE_SIZE,
    SEARCH_DEBOUNCE_MS,
    createBoardState,
    hrefWithSort,
    sortFromSearch,
    type BoardApi,
} from './state.svelte';

const project = { id: 'p1', name: 'Demo', slug: 'demo' } as Project;
const post = (id: string, extra: Partial<PostDetail> = {}) =>
    ({ id, title: `Post ${id}`, vote_count: 0, user_voted: false, ...extra }) as PostDetail;
const pageOf = (posts: PostDetail[], total = posts.length) => ({
    data: posts,
    pagination: { page: 1, per_page: PAGE_SIZE, total, total_pages: 1 },
});

/** A promise the test settles by hand, to order concurrent responses. */
function deferred<T>() {
    let resolve!: (v: T) => void;
    let reject!: (e: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function fakeApi(overrides: Partial<BoardApi> = {}): BoardApi & { [K in keyof BoardApi]: ReturnType<typeof vi.fn> } {
    return {
        getProject: vi.fn(async () => project),
        listPosts: vi.fn(async () => pageOf([post('a'), post('b')])),
        getProjectCounts: vi.fn(async () => ({ by_status: { open: 2 }, by_category: { bug: 1 } })),
        toggleVote: vi.fn(async () => ({ voted: true, vote_count: 1 })),
        createPost: vi.fn(async () => ({})),
        ...overrides,
    } as never;
}

afterEach(() => vi.useRealTimers());

describe('loading', () => {
    it('loads the project and first page, then the counts', async () => {
        const api = fakeApi();
        const board = createBoardState(api);
        expect(board.loading).toBe(true);
        await board.load('demo');
        await Promise.resolve();
        expect(board.project).toEqual(project);
        expect(board.posts.map((p) => p.id)).toEqual(['a', 'b']);
        expect(board.total).toBe(2);
        expect(board.loading).toBe(false);
        expect(board.counts?.by_status.open).toBe(2);
        expect(api.listPosts).toHaveBeenCalledWith('demo', expect.objectContaining({ page: 1, per_page: PAGE_SIZE }));
    });

    it('sends the active filters and sort', async () => {
        const api = fakeApi();
        const board = createBoardState(api);
        board.sort = 'trending';
        board.toggleStatus('planned');
        board.toggleCategory('bug');
        board.searchQuery = 'dark';
        await board.load('demo');
        expect(api.listPosts).toHaveBeenCalledWith('demo', {
            sort: 'trending',
            status: 'planned',
            category: 'bug',
            q: 'dark',
            per_page: PAGE_SIZE,
            page: 1,
        });
    });

    it('a stale response never overwrites a newer one (#173)', async () => {
        const slow = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi.fn().mockReturnValueOnce(slow.promise).mockResolvedValueOnce(pageOf([post('new')]));
        const board = createBoardState(fakeApi({ listPosts }));

        const first = board.load('demo'); // slow, issued first
        await board.load('demo'); // fast, issued second, lands first
        expect(board.posts.map((p) => p.id)).toEqual(['new']);

        slow.resolve(pageOf([post('old')])); // the old answer arrives late
        await first;
        expect(board.posts.map((p) => p.id)).toEqual(['new']);
        expect(board.loading).toBe(false);
        expect(board.refetching).toBe(false);
    });

    it('a stale failure does not show an error either', async () => {
        const slow = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi.fn().mockReturnValueOnce(slow.promise).mockResolvedValueOnce(pageOf([post('ok')]));
        const board = createBoardState(fakeApi({ listPosts }));
        const first = board.load('demo');
        await board.load('demo');
        slow.reject(new Error('boom'));
        await first;
        expect(board.error).toBe('');
        expect(board.posts).toHaveLength(1);
    });

    it('dims instead of showing skeletons when refetching with posts on screen (#148)', async () => {
        const slow = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi.fn().mockResolvedValueOnce(pageOf([post('a')])).mockReturnValueOnce(slow.promise);
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        const again = board.load('demo');
        expect(board.refetching).toBe(true);
        expect(board.loading).toBe(false);
        slow.resolve(pageOf([post('b')]));
        await again;
        expect(board.refetching).toBe(false);
    });

    it('maps a 404 to "Project not found" and anything else to a generic error', async () => {
        const missing = createBoardState(
            fakeApi({ getProject: vi.fn().mockRejectedValue(new ApiError(404, 'nope')) }),
        );
        await missing.load('nope');
        expect(missing.error).toBe('Project not found');
        expect(missing.loading).toBe(false);

        const broken = createBoardState(fakeApi({ listPosts: vi.fn().mockRejectedValue(new ApiError(500, 'x')) }));
        await broken.load('demo');
        expect(broken.error).toBe('Failed to load board');
    });

    it('a counts failure does not break the board', async () => {
        const board = createBoardState(fakeApi({ getProjectCounts: vi.fn().mockRejectedValue(new Error('down')) }));
        await board.load('demo');
        await Promise.resolve();
        expect(board.error).toBe('');
        expect(board.posts).toHaveLength(2);
        expect(board.counts).toBeNull();
    });

    it('late counts from a superseded load are ignored', async () => {
        const lateCounts = deferred<{ by_status: Record<string, number>; by_category: Record<string, number> }>();
        const getProjectCounts = vi
            .fn()
            .mockReturnValueOnce(lateCounts.promise)
            .mockResolvedValueOnce({ by_status: { open: 9 }, by_category: {} });
        const board = createBoardState(fakeApi({ getProjectCounts }));
        await board.load('demo');
        await board.load('demo');
        await Promise.resolve();
        lateCounts.resolve({ by_status: { open: 1 }, by_category: {} });
        await Promise.resolve();
        expect(board.counts?.by_status.open).toBe(9);
    });
});

describe('paging', () => {
    const many = (from: number, n: number) => Array.from({ length: n }, (_, i) => post(`p${from + i}`));

    it('"show more" appends the next page without duplicates', async () => {
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 3), 5))
            // Shifted sort order: the second page repeats p2.
            .mockResolvedValueOnce(pageOf([post('p2'), post('p3'), post('p4')], 5));
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        await board.loadMore('demo');
        expect(board.posts.map((p) => p.id)).toEqual(['p0', 'p1', 'p2', 'p3', 'p4']);
        expect(listPosts).toHaveBeenLastCalledWith('demo', expect.objectContaining({ page: 2 }));
    });

    it('asks for consecutive pages even when de-duplication shortens the list', async () => {
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 6))
            .mockResolvedValueOnce(pageOf([post('p1'), post('p2')], 6)) // one duplicate
            .mockResolvedValueOnce(pageOf([post('p3'), post('p4')], 6));
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        await board.loadMore('demo');
        await board.loadMore('demo');
        const pages = listPosts.mock.calls.map((c) => c[1].page);
        expect(pages).toEqual([1, 2, 3]);
    });

    it('does nothing once every post is shown', async () => {
        const api = fakeApi({ listPosts: vi.fn(async () => pageOf(many(0, 2), 2)) });
        const board = createBoardState(api);
        await board.load('demo');
        await board.loadMore('demo');
        expect(api.listPosts).toHaveBeenCalledTimes(1);
    });

    it('changing a filter starts over at page 1', async () => {
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 4))
            .mockResolvedValueOnce(pageOf(many(2, 2), 4))
            .mockResolvedValueOnce(pageOf([post('bug1')], 1));
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        await board.loadMore('demo');
        expect(board.posts).toHaveLength(4);

        board.toggleCategory('bug');
        await board.load('demo');
        expect(board.posts.map((p) => p.id)).toEqual(['bug1']);
        expect(listPosts).toHaveBeenLastCalledWith('demo', expect.objectContaining({ category: 'bug', page: 1 }));
    });

    it('after a reload "show more" continues from page 2, not from where it was', async () => {
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 8))
            .mockResolvedValueOnce(pageOf(many(2, 2), 8))
            .mockResolvedValueOnce(pageOf(many(4, 2), 8)) // page 3
            .mockResolvedValueOnce(pageOf(many(100, 2), 8)) // reload, page 1
            .mockResolvedValueOnce(pageOf(many(102, 2), 8)); // the next "show more"
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        await board.loadMore('demo');
        await board.loadMore('demo');
        board.toggleStatus('open');
        await board.load('demo');
        await board.loadMore('demo');
        expect(listPosts).toHaveBeenLastCalledWith('demo', expect.objectContaining({ status: 'open', page: 2 }));
    });

    it('"show more" is ignored while the board is reloading', async () => {
        const slowReload = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi.fn().mockResolvedValueOnce(pageOf(many(0, 2), 6)).mockReturnValueOnce(slowReload.promise);
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        const reload = board.load('demo'); // a filter changed: the reload is in flight
        await vi.waitFor(() => expect(listPosts).toHaveBeenCalledTimes(2)); // it has reached the server
        await board.loadMore('demo'); // clicked while the list is dimmed
        expect(listPosts).toHaveBeenCalledTimes(2); // no extra request
        slowReload.resolve(pageOf(many(10, 2), 6));
        await reload;
        expect(board.posts.map((p) => p.id)).toEqual(['p10', 'p11']);
    });

    it('stops offering "show more" when a page brings nothing new', async () => {
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 5)) // total claims 5, only 2 are real
            .mockResolvedValueOnce(pageOf([post('p0'), post('p1')], 5)) // all duplicates
            .mockResolvedValueOnce(pageOf(many(0, 2), 5)); // the reload
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        expect(board.hasMore).toBe(true);
        await board.loadMore('demo');
        expect(board.hasMore).toBe(false);
        await board.loadMore('demo');
        expect(listPosts).toHaveBeenCalledTimes(2); // not asked again
        await board.load('demo'); // a reload starts fresh
        expect(board.hasMore).toBe(true);
    });

    it('a "show more" answer that lands after a reload is dropped', async () => {
        const slowMore = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 4))
            .mockReturnValueOnce(slowMore.promise)
            .mockResolvedValueOnce(pageOf([post('fresh')], 1));
        const board = createBoardState(fakeApi({ listPosts }));
        await board.load('demo');
        const more = board.loadMore('demo');
        await board.load('demo'); // filters changed meanwhile
        slowMore.resolve(pageOf(many(2, 2), 4));
        await more;
        expect(board.posts.map((p) => p.id)).toEqual(['fresh']);
    });

    it('a failed "show more" keeps the board and reports once', async () => {
        const onLoadMoreError = vi.fn();
        const listPosts = vi
            .fn()
            .mockResolvedValueOnce(pageOf(many(0, 2), 4))
            .mockRejectedValueOnce(new Error('offline'));
        const board = createBoardState(fakeApi({ listPosts }), { onLoadMoreError });
        await board.load('demo');
        await board.loadMore('demo');
        expect(board.posts).toHaveLength(2);
        expect(board.error).toBe('');
        expect(onLoadMoreError).toHaveBeenCalledTimes(1);
    });
});

describe('sort in the URL (#203)', () => {
    it('reads ?sort= and falls back to newest for anything unknown', () => {
        expect(sortFromSearch('?sort=trending')).toBe('trending');
        expect(sortFromSearch('?sort=most_votes&x=1')).toBe('most_votes');
        expect(sortFromSearch('?sort=bogus')).toBe('newest');
        expect(sortFromSearch('')).toBe('newest');
    });

    it('writes ?sort= back, drops it for the default and keeps other params', () => {
        const base = 'https://x.test/board/demo?foo=1';
        expect(hrefWithSort(base, 'trending')).toBe('https://x.test/board/demo?foo=1&sort=trending');
        expect(hrefWithSort('https://x.test/board/demo?sort=trending&foo=1', 'newest')).toBe(
            'https://x.test/board/demo?foo=1',
        );
        expect(hrefWithSort('https://x.test/board/demo', 'newest')).toBe('https://x.test/board/demo');
    });

    it('round-trips every sort option', () => {
        for (const sort of ['newest', 'trending', 'most_votes', 'recently_updated'] as const) {
            expect(sortFromSearch(new URL(hrefWithSort('https://x.test/b', sort)).search)).toBe(sort);
        }
    });
});

describe('queueReload', () => {
    it('reloads at once for filter changes and after a pause for typing', async () => {
        vi.useFakeTimers();
        const api = fakeApi();
        const board = createBoardState(api);

        board.queueReload('demo');
        await vi.advanceTimersByTimeAsync(0);
        expect(api.listPosts).toHaveBeenCalledTimes(1);

        board.searchQuery = 'dar';
        board.queueReload('demo');
        board.searchQuery = 'dark';
        board.queueReload('demo');
        await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS - 1);
        expect(api.listPosts).toHaveBeenCalledTimes(1);
        await vi.advanceTimersByTimeAsync(1);
        expect(api.listPosts).toHaveBeenCalledTimes(2);
        expect(api.listPosts).toHaveBeenLastCalledWith('demo', expect.objectContaining({ q: 'dark' }));
    });

    it('the returned canceller stops a pending reload', async () => {
        vi.useFakeTimers();
        const api = fakeApi();
        const board = createBoardState(api);
        board.searchQuery = 'x';
        const cancel = board.queueReload('demo');
        cancel();
        await vi.advanceTimersByTimeAsync(1000);
        expect(api.listPosts).not.toHaveBeenCalled();
    });
});

describe('actions', () => {
    it('voting updates the post in place; a failed vote changes nothing', async () => {
        const board = createBoardState(fakeApi());
        await board.load('demo');
        const first = board.posts[0];
        await board.toggleVote(first);
        expect([first.user_voted, first.vote_count]).toEqual([true, 1]);

        const failing = createBoardState(fakeApi({ toggleVote: vi.fn().mockRejectedValue(new Error('x')) }));
        await failing.load('demo');
        await failing.toggleVote(failing.posts[0]);
        expect(failing.posts[0].user_voted).toBe(false);
    });

    it('creating a post refreshes the board without making the caller wait for it', async () => {
        const slowReload = deferred<ReturnType<typeof pageOf>>();
        const listPosts = vi.fn().mockResolvedValueOnce(pageOf([post('a')])).mockReturnValueOnce(slowReload.promise);
        const api = fakeApi({ listPosts });
        const board = createBoardState(api);
        await board.load('demo');

        await board.createPost('demo', { title: 'T', description: '', category: 'bug' }); // resolves at once
        expect(api.createPost).toHaveBeenCalledWith('demo', { title: 'T', description: '', category: 'bug' });
        await vi.waitFor(() => expect(listPosts).toHaveBeenCalledTimes(2)); // the refresh started
        expect(board.refetching).toBe(true); // …and is still running

        slowReload.resolve(pageOf([post('new'), post('a')]));
        await vi.waitFor(() => expect(board.posts.map((p) => p.id)).toEqual(['new', 'a']));
    });

    it('a failed create leaves the board alone and tells the caller', async () => {
        const api = fakeApi({ createPost: vi.fn().mockRejectedValue(new Error('nope')) });
        const board = createBoardState(api);
        await board.load('demo');
        await expect(board.createPost('demo', { title: 'T', description: '', category: 'bug' })).rejects.toThrow('nope');
        expect(api.listPosts).toHaveBeenCalledTimes(1);
    });

    it('filters toggle, clear and report whether any are active', () => {
        const board = createBoardState(fakeApi());
        expect(board.hasFilters).toBe(false);
        board.toggleStatus('open');
        expect(board.statusFilter).toBe('open');
        board.toggleStatus('open');
        expect(board.statusFilter).toBe('');
        board.searchQuery = 'q';
        board.toggleCategory('bug');
        expect(board.hasFilters).toBe(true);
        board.clearFilters();
        expect(board.hasFilters).toBe(false);
    });

    it('j / k move the focus within the posts on screen and reset on reload', async () => {
        const board = createBoardState(fakeApi());
        expect(board.focusNext()).toBe(false); // nothing to focus yet
        await board.load('demo');
        board.focusNext();
        board.focusNext();
        board.focusNext(); // clamps at the last post
        expect(board.focusedPost?.id).toBe('b');
        board.focusPrev();
        board.focusPrev(); // clamps at the first
        expect(board.focusedPost?.id).toBe('a');
        await board.load('demo');
        expect(board.focusedIndex).toBe(-1);
    });
});
