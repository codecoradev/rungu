// Post-detail state — what the post page does with data, apart from drawing it.
//
// Like the board (see $lib/board/state.svelte.ts) this takes `api` as an
// injected dependency so it can be tested with a fake, and the page only
// renders and forwards events. It also owns the one thing the page used to get
// wrong: loading again when the post id changes (a "Similar posts" link goes to
// another post on the *same* route, which reuses the component and never re-ran
// the load, so the URL changed but the old post stayed on screen).

import { ApiError } from '$lib/api/client';
import type { Comment, CurrentUser, PostCategory, PostDetail, PostStatus } from '$lib/api/types';

/** The part of the REST client the post page uses. */
export interface PostApi {
    getPost(id: string): Promise<PostDetail>;
    listComments(postId: string): Promise<Comment[]>;
    getOfficialResponse(postId: string): Promise<Comment | null>;
    getSimilarPosts(postId: string): Promise<PostDetail[]>;
    createComment(postId: string, body: { content: string; parent_id?: string }): Promise<Comment>;
    deleteComment(id: string): Promise<unknown>;
    updatePostStatus(id: string, status: PostStatus): Promise<PostDetail>;
    updatePostCategory(id: string, category: PostCategory): Promise<PostDetail>;
    setOfficialResponse(postId: string, commentId: string | null): Promise<{ official_response: Comment | null }>;
}

/** Where messages to the person go (the app uses toasts). */
export interface Notifier {
    success(message: string): void;
    error(message: string): void;
}

/**
 * Who may change a post's status and category, and manage its attachments:
 * an admin or the post's author. This mirrors `Actor::require_owner_or_admin`
 * in rungu-core, which is what actually enforces it — the client copy only
 * decides which controls to show. Kept in this one place.
 */
export function canModerate(user: CurrentUser | null, post: Pick<PostDetail, 'created_by'> | null): boolean {
    if (!user || !post) return false;
    return user.role === 'admin' || user.id === post.created_by;
}

/**
 * The id of `rootId` plus every comment whose `parent_id` chain leads back to
 * it. Used after a delete to prune orphaned replies locally; the server stays
 * the source of truth.
 */
export function collectDescendants(list: Comment[], rootId: string): Set<string> {
    const ids = new Set<string>([rootId]);
    let added = true;
    while (added) {
        added = false;
        for (const c of list) {
            if (c.parent_id && ids.has(c.parent_id) && !ids.has(c.id)) {
                ids.add(c.id);
                added = true;
            }
        }
    }
    return ids;
}

export function createPostState(api: PostApi, deps: { user: () => CurrentUser | null; notify: Notifier }) {
    let post = $state<PostDetail | null>(null);
    let comments = $state<Comment[]>([]);
    let official = $state<Comment | null>(null);
    let similar = $state<PostDetail[]>([]);
    let loading = $state(true);
    // Blocking load error: renders the whole error page.
    let error = $state('');
    let commentText = $state('');
    let replyTo = $state<string | null>(null);
    let commentLoading = $state(false);
    let officialBusy = $state(false);

    // Bumped by every load. Only the latest may commit, and anything that was
    // started for an earlier post (a comment, a status change) is dropped when
    // it lands after the person moved on to another one.
    let generation = 0;
    let currentId = '';

    /** Load a post, its comments, the official response and similar posts. */
    async function load(postId: string): Promise<void> {
        const mine = ++generation;
        currentId = postId;
        loading = true;
        error = '';
        post = null;
        comments = [];
        official = null;
        similar = [];
        replyTo = null;
        commentText = '';
        try {
            const loaded = await api.getPost(postId);
            const loadedComments = await api.listComments(postId);
            if (mine !== generation) return;
            post = loaded;
            comments = loadedComments;
            // Best-effort: their absence never blocks the post.
            api.getOfficialResponse(postId)
                .then((r) => {
                    if (mine === generation) official = r;
                })
                .catch(() => {});
            api.getSimilarPosts(postId)
                .then((r) => {
                    if (mine === generation) similar = r;
                })
                .catch(() => {});
        } catch (e) {
            if (mine !== generation) return;
            error = e instanceof ApiError && e.status === 404 ? 'Post not found' : 'Failed to load post';
        } finally {
            if (mine === generation) loading = false;
        }
    }

    async function submitComment(): Promise<void> {
        const content = commentText.trim();
        if (!content || commentLoading) return;
        const mine = generation;
        commentLoading = true;
        try {
            const created = await api.createComment(currentId, { content, parent_id: replyTo ?? undefined });
            if (mine !== generation) return;
            comments = [...comments, created];
            commentText = '';
            replyTo = null;
        } catch {
            if (mine === generation) deps.notify.error('Failed to post comment');
        } finally {
            if (mine === generation) commentLoading = false;
        }
    }

    async function deleteComment(id: string): Promise<void> {
        const mine = generation;
        try {
            await api.deleteComment(id);
            if (mine !== generation) return;
            // Remove the comment and all of its descendants: the server cascades,
            // and without this the replies would vanish while still being counted.
            const gone = collectDescendants(comments, id);
            comments = comments.filter((c) => !gone.has(c.id));
        } catch {
            if (mine === generation) deps.notify.error('Failed to delete comment');
        }
    }

    async function setOfficialResponse(commentId: string | null): Promise<void> {
        if (!post || officialBusy) return;
        const mine = generation;
        officialBusy = true;
        try {
            const res = await api.setOfficialResponse(post.id, commentId);
            if (mine !== generation) return;
            official = res.official_response;
            deps.notify.success(commentId ? 'Official response pinned ✓' : 'Official response removed ✓');
        } catch {
            if (mine === generation) deps.notify.error('Failed to update official response');
        } finally {
            if (mine === generation) officialBusy = false;
        }
    }

    /** Returns whether the change was saved, so the caller can reset its control. */
    async function changeStatus(status: PostStatus): Promise<boolean> {
        if (!post) return false;
        const mine = generation;
        try {
            const updated = await api.updatePostStatus(post.id, status);
            if (mine !== generation) return true;
            post = updated;
            deps.notify.success(`Status updated: ${status.replace('_', ' ')} ✓`);
            return true;
        } catch {
            if (mine === generation) deps.notify.error('Failed to update status');
            return false;
        }
    }

    async function changeCategory(category: PostCategory): Promise<boolean> {
        if (!post) return false;
        const mine = generation;
        try {
            const updated = await api.updatePostCategory(post.id, category);
            if (mine !== generation) return true;
            post = updated;
            deps.notify.success(`Category updated: ${category} ✓`);
            return true;
        } catch {
            if (mine === generation) deps.notify.error('Failed to update category');
            return false;
        }
    }

    /** The vote rail reports the outcome of its own request. */
    function applyVote(voted: boolean, count: number) {
        if (post) {
            post.user_voted = voted;
            post.vote_count = count;
        }
    }

    return {
        get post() {
            return post;
        },
        get comments() {
            return comments;
        },
        get official() {
            return official;
        },
        get similar() {
            return similar;
        },
        get loading() {
            return loading;
        },
        get error() {
            return error;
        },
        get commentText() {
            return commentText;
        },
        set commentText(v: string) {
            commentText = v;
        },
        get replyTo() {
            return replyTo;
        },
        get commentLoading() {
            return commentLoading;
        },
        get officialBusy() {
            return officialBusy;
        },
        get isAdmin() {
            return deps.user()?.role === 'admin';
        },
        /** Whether the signed-in person may edit status/category and attachments. */
        get canModerate() {
            return canModerate(deps.user(), post);
        },
        startReply(parentId: string) {
            replyTo = parentId;
        },
        cancelReply() {
            replyTo = null;
        },
        load,
        submitComment,
        deleteComment,
        setOfficialResponse,
        changeStatus,
        changeCategory,
        applyVote,
    };
}

export type PostState = ReturnType<typeof createPostState>;
