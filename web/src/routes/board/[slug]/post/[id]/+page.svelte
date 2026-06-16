<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { PostDetail, Comment, CurrentUser, PostStatus, PostCategory } from '$lib/api/types';
    import StatusBadge from '$lib/components/StatusBadge.svelte';
    import CategoryBadge from '$lib/components/CategoryBadge.svelte';
    import VoteButton from '$lib/components/VoteButton.svelte';
    import CommentThread from '$lib/components/CommentThread.svelte';
    import { Button } from '$lib/components/ui/button';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import { timeAgo } from '$lib/utils';

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
    const categoryOptions: PostCategory[] = ['feedback', 'bug', 'feature', 'question'];
    const isAdmin = $derived(user?.role === 'admin');
    const isAuthor = $derived(user?.id === post?.created_by);
    const canEditStatus = $derived(isAdmin || isAuthor);

    async function loadData() {
        loading = true;
        try {
            post = await api.getPost(postId);
            comments = await api.listComments(postId);
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
        } catch {
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

    async function handleCategoryChange(e: Event) {
        const target = e.currentTarget as HTMLSelectElement;
        const category = target.value as PostCategory;
        if (!post) return;
        try {
            post = await api.updatePostCategory(post.id, category);
        } catch {
            error = 'Failed to update category';
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
    <Card.Root>
        <Card.Header class="h-48 animate-pulse bg-muted" />
    </Card.Root>
{:else if error}
    <div class="py-16 text-center">
        <p class="text-lg text-muted-foreground">{error}</p>
        <Button variant="link" href={`/board/${slug}`}>← Back to board</Button>
    </div>
{:else if post}
    <Button variant="link" size="sm" href={`/board/${slug}`} class="px-0 text-muted-foreground">
        ← Back to board
    </Button>

    <Card.Root class="mt-3">
        <Card.Header class="flex-row items-start gap-4">
            <div class="shrink-0">
                <VoteButton postId={post.id} voted={post.user_voted} count={post.vote_count} onvote={handleVote} />
            </div>
            <div class="min-w-0 flex-1">
                <div class="mb-2 flex flex-wrap items-center gap-2">
                    <CategoryBadge category={post.category} />
                    {#if canEditStatus}
                        <select
                            value={post.category}
                            onchange={handleCategoryChange}
                            class="rounded-md border border-input bg-background px-2 py-0.5 text-xs capitalize"
                        >
                            {#each categoryOptions as c (c)}
                                <option value={c} class="capitalize">{c}</option>
                            {/each}
                        </select>
                    {/if}
                    <StatusBadge status={post.status} />
                    {#if canEditStatus}
                        <select
                            value={post.status}
                            onchange={handleStatusChange}
                            class="rounded-md border border-input bg-background px-2 py-0.5 text-xs capitalize"
                        >
                            {#each statusOptions as s (s)}
                                <option value={s} class="capitalize">{s.replace('_', ' ')}</option>
                            {/each}
                        </select>
                    {/if}
                </div>
                <Card.Title class="text-xl">{post.title}</Card.Title>
                <div class="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                    <span>{post.creator.name || post.creator.email}</span>
                    <span>·</span>
                    <span>{timeAgo(post.created_at)}</span>
                </div>
            </div>
        </Card.Header>
        {#if post.description}
            <Card.Content>
                <div class="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">
                    {post.description}
                </div>
            </Card.Content>
        {/if}
    </Card.Root>

    <section class="mt-6">
        <h2 class="mb-4 text-sm font-semibold">Comments ({comments.length})</h2>

        {#if user}
            <form onsubmit={handleComment} class="mb-6">
                {#if replyTo}
                    {@const parentComment = comments.find((c) => c.id === replyTo)}
                    {#if parentComment}
                        <div class="mb-2 rounded-lg border border-border bg-muted/50 p-3">
                            <div class="flex items-center gap-2 text-xs text-muted-foreground">
                                <span class="font-medium">{parentComment.creator?.name || parentComment.creator?.email || 'User'}</span>
                                <span>·</span>
                                <span>{timeAgo(parentComment.created_at)}</span>
                            </div>
                            <p class="mt-1 line-clamp-2 text-xs text-muted-foreground">{parentComment.content}</p>
                        </div>
                    {/if}
                    <div class="mb-1 text-xs text-muted-foreground">
                        Replying to thread ·
                        <button type="button" onclick={() => (replyTo = null)} class="text-primary">cancel</button>
                    </div>
                {/if}
                <Textarea
                    bind:value={commentText}
                    placeholder={replyTo ? 'Write a reply...' : 'Share your thoughts...'}
                    rows="3"
                />
                <div class="mt-2 flex justify-end">
                    <Button type="submit" disabled={!commentText.trim() || commentLoading}>
                        {commentLoading ? 'Posting...' : 'Comment'}
                    </Button>
                </div>
            </form>
        {:else}
            <Card.Root class="mb-6">
                <Card.Content class="pt-6 text-center text-sm text-muted-foreground">
                    <a href="/login" class="font-medium text-primary hover:underline">Login</a> to join the discussion
                </Card.Content>
            </Card.Root>
        {/if}

        <CommentThread
            {comments}
            postId={post.id}
            currentUserId={user?.id}
            onreply={(parentId) => (replyTo = parentId)}
            ondelete={handleDeleteComment}
        />
    </section>
{/if}
