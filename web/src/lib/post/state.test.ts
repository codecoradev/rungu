import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/client';
import type { Comment, PostDetail, PostPermissions, PostView } from '$lib/api/types';
import { collectDescendants, createPostState, type PostApi } from './state.svelte';

const none: PostPermissions = { can_edit: false, can_moderate: false };
const post = (id: string, extra: Partial<PostView> = {}) =>
    ({
        id,
        title: `Post ${id}`,
        status: 'open',
        category: 'feedback',
        created_by: 'author',
        user_voted: false,
        vote_count: 0,
        permissions: none,
        ...extra,
    }) as PostView;
const comment = (id: string, parent_id?: string) => ({ id, content: `c${id}`, parent_id }) as Comment;

function deferred<T>() {
    let resolve!: (v: T) => void;
    let reject!: (e: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function fakeApi(overrides: Partial<PostApi> = {}) {
    return {
        getPost: vi.fn(async (id: string) => post(id)),
        listComments: vi.fn(async () => [comment('c1'), comment('c2', 'c1')]),
        getOfficialResponse: vi.fn(async () => null),
        getSimilarPosts: vi.fn(async () => [post('s1')]),
        createComment: vi.fn(async (_id: string, b: { content: string }) => comment('new')),
        deleteComment: vi.fn(async () => undefined),
        updatePostStatus: vi.fn(async (id: string, status: string) => post(id, { status } as never)),
        updatePostCategory: vi.fn(async (id: string, category: string) => post(id, { category } as never)),
        setOfficialResponse: vi.fn(async (_id: string, c: string | null) => ({
            official_response: c ? comment(c) : null,
        })),
        ...overrides,
    } as unknown as PostApi & Record<keyof PostApi, ReturnType<typeof vi.fn>>;
}

function setup(apiOverrides: Partial<PostApi> = {}) {
    const api = fakeApi(apiOverrides);
    const notify = { success: vi.fn(), error: vi.fn() };
    const state = createPostState(api, { notify });
    return { api, notify, state };
}

describe('collectDescendants', () => {
    it('finds replies at any depth, and nothing else', () => {
        const list = [comment('a'), comment('b', 'a'), comment('c', 'b'), comment('d'), comment('e', 'd')];
        expect([...collectDescendants(list, 'a')].sort()).toEqual(['a', 'b', 'c']);
        expect([...collectDescendants(list, 'e')]).toEqual(['e']);
    });
});

describe('load', () => {
    it('loads the post and comments, then the optional extras', async () => {
        const { state, api } = setup({ getOfficialResponse: vi.fn(async () => comment('pin')) });
        expect(state.loading).toBe(true);
        await state.load('p1');
        await vi.waitFor(() => expect(state.official?.id).toBe('pin'));
        expect(state.post?.id).toBe('p1');
        expect(state.comments).toHaveLength(2);
        expect(state.similar.map((p) => p.id)).toEqual(['s1']);
        expect(state.loading).toBe(false);
        expect(api.getPost).toHaveBeenCalledWith('p1');
    });

    it('maps 404 to "Post not found" and anything else to a generic error', async () => {
        const missing = setup({ getPost: vi.fn().mockRejectedValue(new ApiError(404, 'x')) });
        await missing.state.load('nope');
        expect(missing.state.error).toBe('Post not found');
        expect(missing.state.loading).toBe(false);

        const broken = setup({ listComments: vi.fn().mockRejectedValue(new ApiError(500, 'x')) });
        await broken.state.load('p1');
        expect(broken.state.error).toBe('Failed to load post');
    });

    it('missing extras never block the post', async () => {
        const { state } = setup({
            getOfficialResponse: vi.fn().mockRejectedValue(new Error('x')),
            getSimilarPosts: vi.fn().mockRejectedValue(new Error('x')),
        });
        await state.load('p1');
        await Promise.resolve();
        expect(state.error).toBe('');
        expect(state.post?.id).toBe('p1');
        expect(state.official).toBeNull();
        expect(state.similar).toEqual([]);
    });

    it('opening another post replaces the first completely (the "Similar posts" link case)', async () => {
        const { state } = setup({
            getSimilarPosts: vi.fn(async (id: string) => [post(`${id}-similar`)]),
            getOfficialResponse: vi.fn(async (id: string) => (id === 'p1' ? comment('pin1') : null)),
        });
        await state.load('p1');
        await vi.waitFor(() => expect(state.official?.id).toBe('pin1'));
        state.startReply('c1');
        state.commentText = 'half-typed';

        await state.load('p2');
        await vi.waitFor(() => expect(state.similar[0]?.id).toBe('p2-similar'));
        expect(state.post?.id).toBe('p2');
        expect(state.official).toBeNull(); // p1's pin does not leak onto p2
        expect(state.replyTo).toBeNull();
        expect(state.commentText).toBe('');
    });

    it('a slow answer for the previous post never overwrites the current one', async () => {
        const slow = deferred<PostDetail>();
        const getPost = vi.fn().mockReturnValueOnce(slow.promise).mockImplementationOnce(async (id: string) => post(id));
        const { state } = setup({ getPost });
        const first = state.load('p1');
        await state.load('p2');
        slow.resolve(post('p1'));
        await first;
        expect(state.post?.id).toBe('p2');
        expect(state.loading).toBe(false);
    });

    it('a slow failure for the previous post shows no error', async () => {
        const slow = deferred<PostDetail>();
        const getPost = vi.fn().mockReturnValueOnce(slow.promise).mockImplementationOnce(async (id: string) => post(id));
        const { state } = setup({ getPost });
        const first = state.load('p1');
        await state.load('p2');
        slow.reject(new ApiError(404, 'gone'));
        await first;
        expect(state.error).toBe('');
        expect(state.post?.id).toBe('p2');
    });

    it('late extras from the previous post are dropped', async () => {
        const lateSimilar = deferred<PostDetail[]>();
        const getSimilarPosts = vi
            .fn()
            .mockReturnValueOnce(lateSimilar.promise)
            .mockResolvedValueOnce([post('for-p2')]);
        const { state } = setup({ getSimilarPosts });
        await state.load('p1');
        await state.load('p2');
        await vi.waitFor(() => expect(state.similar.map((p) => p.id)).toEqual(['for-p2']));
        lateSimilar.resolve([post('for-p1')]);
        await Promise.resolve();
        expect(state.similar.map((p) => p.id)).toEqual(['for-p2']);
    });
});

describe('comments', () => {
    it('posts a trimmed comment, as a reply when one is targeted, then clears the composer', async () => {
        const { state, api } = setup();
        await state.load('p1');
        state.startReply('c1');
        state.commentText = '  hello  ';
        await state.submitComment();
        expect(api.createComment).toHaveBeenCalledWith('p1', { content: 'hello', parent_id: 'c1' });
        expect(state.comments.map((c) => c.id)).toEqual(['c1', 'c2', 'new']);
        expect(state.commentText).toBe('');
        expect(state.replyTo).toBeNull();
    });

    it('ignores blank text and a second submit while one is in flight', async () => {
        const slow = deferred<Comment>();
        const createComment = vi.fn().mockReturnValueOnce(slow.promise);
        const { state } = setup({ createComment });
        await state.load('p1');
        state.commentText = '   ';
        await state.submitComment();
        expect(createComment).not.toHaveBeenCalled();

        state.commentText = 'once';
        const first = state.submitComment();
        expect(state.commentLoading).toBe(true);
        await state.submitComment();
        expect(createComment).toHaveBeenCalledTimes(1);
        slow.resolve(comment('x'));
        await first;
        expect(state.commentLoading).toBe(false);
    });

    it('a failed comment keeps the text and tells the person', async () => {
        const { state, notify } = setup({ createComment: vi.fn().mockRejectedValue(new Error('x')) });
        await state.load('p1');
        state.commentText = 'keep me';
        await state.submitComment();
        expect(state.commentText).toBe('keep me');
        expect(state.comments).toHaveLength(2);
        expect(notify.error).toHaveBeenCalledWith('Failed to post comment');
    });

    it('a comment that lands after moving to another post is dropped', async () => {
        const slow = deferred<Comment>();
        const { state } = setup({ createComment: vi.fn().mockReturnValueOnce(slow.promise) });
        await state.load('p1');
        state.commentText = 'late';
        const pending = state.submitComment();
        await state.load('p2');
        slow.resolve(comment('late-comment'));
        await pending;
        expect(state.comments.map((c) => c.id)).toEqual(['c1', 'c2']); // p2's own, not the late one
    });

    it('deleting removes the comment and its replies', async () => {
        const { state, api } = setup();
        await state.load('p1');
        await state.deleteComment('c1');
        expect(api.deleteComment).toHaveBeenCalledWith('c1');
        expect(state.comments).toEqual([]);
    });

    it('a failed delete keeps the comments and tells the person', async () => {
        const { state, notify } = setup({ deleteComment: vi.fn().mockRejectedValue(new Error('x')) });
        await state.load('p1');
        await state.deleteComment('c1');
        expect(state.comments).toHaveLength(2);
        expect(notify.error).toHaveBeenCalledWith('Failed to delete comment');
    });
});

describe('moderation', () => {
    it('changes status and category, reporting whether they were saved', async () => {
        const { state, notify } = setup();
        await state.load('p1');
        expect(await state.changeStatus('planned')).toBe(true);
        expect(state.post?.status).toBe('planned');
        expect(notify.success).toHaveBeenCalledWith('Status updated: planned ✓');
        expect(await state.changeCategory('bug')).toBe(true);
        expect(state.post?.category).toBe('bug');
    });

    it('a failed change returns false so the caller can reset its control', async () => {
        const { state, notify } = setup({
            updatePostStatus: vi.fn().mockRejectedValue(new Error('x')),
            updatePostCategory: vi.fn().mockRejectedValue(new Error('x')),
        });
        await state.load('p1');
        expect(await state.changeStatus('done')).toBe(false);
        expect(await state.changeCategory('bug')).toBe(false);
        expect(state.post?.status).toBe('open');
        expect(notify.error).toHaveBeenCalledWith('Failed to update status');
        expect(notify.error).toHaveBeenCalledWith('Failed to update category');
    });

    it('pins and unpins the official response', async () => {
        const { state, notify } = setup();
        await state.load('p1');
        await state.setOfficialResponse('c1');
        expect(state.official?.id).toBe('c1');
        expect(notify.success).toHaveBeenCalledWith('Official response pinned ✓');
        await state.setOfficialResponse(null);
        expect(state.official).toBeNull();
        expect(notify.success).toHaveBeenCalledWith('Official response removed ✓');
        expect(state.officialBusy).toBe(false);
    });

    it('shows controls from the permissions the server sent, not from a copy of the rule', async () => {
        const cases: [string, PostPermissions][] = [
            ['visitor or stranger', none],
            ['author', { can_edit: true, can_moderate: false }],
            ['admin', { can_edit: true, can_moderate: true }],
        ];
        for (const [who, permissions] of cases) {
            const { state } = setup({ getPost: vi.fn(async (id: string) => post(id, { permissions })) });
            expect(state.canEdit, `${who} before load`).toBe(false);
            await state.load('p1');
            expect(state.canEdit, who).toBe(permissions.can_edit);
            expect(state.canModerate, who).toBe(permissions.can_moderate);
        }
    });

    it('keeps the permissions after an edit (the edit response does not carry them)', async () => {
        const permissions = { can_edit: true, can_moderate: true };
        const { state } = setup({ getPost: vi.fn(async (id: string) => post(id, { permissions })) });
        await state.load('p1');
        await state.changeStatus('planned');
        expect(state.post?.status).toBe('planned');
        expect(state.canEdit && state.canModerate).toBe(true);
        await state.changeCategory('bug');
        expect(state.post?.category).toBe('bug');
        expect(state.canEdit && state.canModerate).toBe(true);
    });

    it('opening another post uses that post\'s permissions', async () => {
        const perms: Record<string, PostPermissions> = {
            mine: { can_edit: true, can_moderate: false },
            theirs: none,
        };
        const { state } = setup({ getPost: vi.fn(async (id: string) => post(id, { permissions: perms[id] })) });
        await state.load('mine');
        expect(state.canEdit).toBe(true);
        await state.load('theirs');
        expect(state.canEdit).toBe(false);
    });

    it('applies the vote the rail reports', async () => {
        const { state } = setup();
        await state.load('p1');
        state.applyVote(true, 7);
        expect([state.post?.user_voted, state.post?.vote_count]).toEqual([true, 7]);
    });
});
