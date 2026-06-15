<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { PostDetail, Comment, CurrentUser, PostStatus } from '$lib/api/types';
    import StatusBadge from '$lib/components/StatusBadge.svelte';
    import CategoryBadge from '$lib/components/CategoryBadge.svelte';
    import VoteButton from '$lib/components/VoteButton.svelte';
    import CommentThread from '$lib/components/CommentThread.svelte';
    import { timeAgo, cn } from '$lib/utils';

    let { params } = $props();
    let slug = $derived(params.slug);
    let postId = $derived(params.id);

    let post = $state<PostDetail | null>(null);
    let comments = $state<Comment[]>([]);
    let user = $state<CurrentUser | null>(null);
    let loading = $state(true);
    let error = $state('');
    let commentText = $state('');
    let replyTo = $state<string | null>(null);
    let commentLoading = $state(false);

    const statusOptions: PostStatus[] = ['open', 'planned', 'in_progress', 'done', 'declined'];
    const isAdmin = $derived(user?.role === 'admin');
    const isAuthor = $derived(user?.id === post?.created_by);
    const canEditStatus = $derived(isAdmin || isAuthor);

    async function loadData() {
        loading = true;
        try {
            post = await api.getPost(postId);
            const [commentList] = await Promise.all([
                api.listComments(postId),
            ]);
            comments = commentList;
        } catch (e) {
            error = e instanceof ApiError && e.status === 404 ? 'Post not found' : 'Failed to load post';
        } finally {
            loading = false;
        }
    }

    onMount(async () => {
        try {
            user = await api.getCurrentUser();
        } catch {
            user = null;
        }
        await loadData();
    });

    async function handleComment(e: Event) {
        e.preventDefault();
        if (!commentText.trim()) return;
        commentLoading = true;
        try {
            const newComment = await api.createComment(postId, {
                content: commentText.trim(),
                parent_id: replyTo ?? undefined,
            });
            comments = [...comments, newComment];
            commentText = '';
            replyTo = null;
        } catch (err) {
            error = 'Failed to post comment';
        } finally {
            commentLoading = false;
        }
    }

    async function handleDeleteComment(id: string) {
        try {
            await api.deleteComment(id);
            comments = comments.filter((c) => c.id !== id);
        } catch {
            error = 'Failed to delete comment';
        }
    }

    async function handleStatusChange(e: Event) {
        const target = e.currentTarget as HTMLSelectElement;
        const status = target.value as PostStatus;
        if (!post) return;
        try {
            post = await api.updatePostStatus(post.id, status);
        } catch {
            error = 'Failed to update status';
        }
    }

    function handleVote(voted: boolean, count: number) {
        if (post) {
            post.user_voted = voted;
            post.vote_count = count;
        }
    }
</script>

<svelte:head>
    <title>{post?.title ?? 'Post'} — Rungu</title>
</svelte:head>

{#if loading}
    <div class="py-16 text-center">
        <div class="inline-block size-8 animate-spin rounded-full border-2 border-gray-200 border-t-brand-600"></div>
    </div>
{:else if error}
    <div class="py-16 text-center">
        <p class="text-lg text-gray-500">{error}</p>
        <a href={`/board/${slug}`} class="mt-3 inline-block text-sm text-brand-600 hover:underline">← Back to board</a>
    </div>
{:else if post}
    <a href={`/board/${slug}`} class="text-sm text-gray-400 hover:text-gray-600">← Back to board</a>

    <article class="mt-3 rounded-xl border border-gray-200 bg-white p-6">
        <!-- Header -->
        <div class="flex items-start gap-4">
            <div class="shrink-0">
                <VoteButton postId={post.id} voted={post.user_voted} count={post.vote_count} onvote={handleVote} />
            </div>
            <div class="min-w-0 flex-1">
                <div class="mb-2 flex flex-wrap items-center gap-2">
                    <CategoryBadge category={post.category} />
                    <StatusBadge status={post.status} />
                    {#if canEditStatus}
                        <select
                            value={post.status}
                            onchange={handleStatusChange}
                            class="rounded-md border border-gray-200 px-2 py-0.5 text-xs text-gray-500"
                        >
                            {#each statusOptions as s (s)}
                                <option value={s} class="capitalize">{s.replace('_', ' ')}</option>
                            {/each}
                        </select>
                    {/if}
                </div>
                <h1 class="text-xl font-bold text-gray-900">{post.title}</h1>
                <div class="mt-2 flex items-center gap-2 text-xs text-gray-400">
                    <span>{post.creator.name || post.creator.email}</span>
                    <span>·</span>
                    <span>{timeAgo(post.created_at)}</span>
                </div>
            </div>
        </div>

        {#if post.description}
            <div class="mt-4 whitespace-pre-wrap text-sm leading-relaxed text-gray-600">
                {post.description}
            </div>
        {/if}
    </article>

    <!-- Comments section -->
    <section class="mt-6">
        <h2 class="mb-4 text-sm font-semibold text-gray-700">
            Comments ({comments.length})
        </h2>

        {#if user}
            <!-- Comment form -->
            <form onsubmit={handleComment} class="mb-6">
                {#if replyTo}
                    <div class="mb-1 text-xs text-gray-400">
                        Replying to thread ·
                        <button type="button" onclick={() => (replyTo = null)} class="text-brand-600">cancel</button>
                    </div>
                {/if}
                <textarea
                    bind:value={commentText}
                    placeholder={replyTo ? 'Write a reply...' : 'Share your thoughts...'}
                    rows="3"
                    class="w-full resize-none rounded-lg border border-gray-200 px-3 py-2 text-sm outline-none focus:border-brand-400"
                ></textarea>
                <div class="mt-2 flex justify-end">
                    <button
                        type="submit"
                        disabled={!commentText.trim() || commentLoading}
                        class="rounded-lg bg-brand-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
                    >
                        {commentLoading ? 'Posting...' : 'Comment'}
                    </button>
                </div>
            </form>
        {:else}
            <div class="mb-6 rounded-lg border border-gray-200 bg-gray-50 p-3 text-center text-sm text-gray-500">
                <a href="/login" class="font-medium text-brand-600 hover:underline">Login</a> to join the discussion
            </div>
        {/if}

        <!-- Threaded comments -->
        <CommentThread
            {comments}
            postId={post.id}
            currentUserId={user?.id}
            onreply={(parentId) => (replyTo = parentId)}
            ondelete={handleDeleteComment}
        />
    </section>
{/if}
