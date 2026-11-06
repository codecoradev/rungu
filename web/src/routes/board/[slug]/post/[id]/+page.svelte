<script lang="ts">
    import { branding } from '$lib/branding.svelte';
    import { onMount, tick, untrack } from 'svelte';
    import CircleAlert from '@lucide/svelte/icons/circle-alert';
    import ShieldCheck from '@lucide/svelte/icons/shield-check';
    import ChevronUp from '@lucide/svelte/icons/chevron-up';
    import { api } from '$lib/api/client';
    import { session } from '$lib/session.svelte';
    import { createPostState } from '$lib/post/state.svelte';
    import type { Comment, PostStatus, PostCategory } from '$lib/api/types';
    import StatusBadge from '$lib/components/StatusBadge.svelte';
    import CategoryBadge from '$lib/components/CategoryBadge.svelte';
    import VoteRail from '$lib/components/VoteRail.svelte';
    import CommentThread from '$lib/components/CommentThread.svelte';
    import AttachmentGallery from '$lib/components/AttachmentGallery.svelte';
    import { Button } from '$lib/components/ui/button';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import { page } from '$app/state';
    import { loginHref, timeAgo } from '$lib/utils';
    import { toastError, toastSuccess } from '$lib/toast.svelte';

    let { params } = $props();
    let slug = $derived(params.slug);
    let postId = $derived(params.id);

    // Everything the page does with data (loading, comments, moderation, the
    // official response) lives in the state module; this page renders it and
    // forwards events. Transient per-action errors go through the toaster (#144).
    const detail = createPostState(api, {
        user: () => session.user,
        notify: { success: toastSuccess, error: toastError },
    });

    const user = $derived(session.user);
    const post = $derived(detail.post);
    const comments = $derived(detail.comments);
    const official = $derived(detail.official);
    const similar = $derived(detail.similar);
    const loading = $derived(detail.loading);
    const error = $derived(detail.error);
    const replyTo = $derived(detail.replyTo);
    const commentLoading = $derived(detail.commentLoading);
    const officialBusy = $derived(detail.officialBusy);
    const isAdmin = $derived(detail.isAdmin);
    const canEditStatus = $derived(detail.canModerate);

    const statusOptions: PostStatus[] = ['open', 'planned', 'in_progress', 'done', 'declined'];
    const categoryOptions: PostCategory[] = ['feedback', 'bug', 'feature', 'question'];

    // Load once the session is known, and again whenever the id changes: a
    // "Similar posts" link goes to another post on this same route, which reuses
    // this component instead of mounting it again.
    let ready = $state(false);
    onMount(async () => {
        await session.ready();
        ready = true;
    });
    $effect(() => {
        if (!ready) return;
        const id = postId;
        untrack(() => void detail.load(id));
    });

    /**
     * Post-detail keyboard-shortcut handler. Currently just `r` → focus reply
     * box (auth-gated). See $lib/shortcuts.ts for the full registry.
     */
    function onShortcut(e: Event) {
        const { key } = (e as CustomEvent).detail as { key: string; scope: string };
        if (key === 'r') {
            if (!user) return;
            const el = document.getElementById('post-reply');
            if (el instanceof HTMLElement) {
                el.focus();
                e.preventDefault(); // handled: don't type the 'r' into the reply box
            }
        }
    }

    onMount(() => {
        const handler = onShortcut as EventListener;
        window.addEventListener('rungu:shortcut', handler);
        return () => window.removeEventListener('rungu:shortcut', handler);
    });

    /** Reply targets a thread and moves focus to the composer above it. */
    async function startReply(parentId: string) {
        detail.startReply(parentId);
        await tick();
        document.getElementById('post-reply')?.focus();
    }

    function handleComment(e: Event) {
        e.preventDefault();
        void detail.submitComment();
    }

    async function handleStatusChange(e: Event) {
        const target = e.currentTarget as HTMLSelectElement;
        const saved = await detail.changeStatus(target.value as PostStatus);
        // Reset the select so it reflects the persisted post, not the failed choice
        if (!saved && detail.post) target.value = detail.post.status;
    }

    async function handleCategoryChange(e: Event) {
        const target = e.currentTarget as HTMLSelectElement;
        const saved = await detail.changeCategory(target.value as PostCategory);
        if (!saved && detail.post) target.value = detail.post.category;
    }
</script>

<svelte:head>
    <title>{(post?.title ?? 'Post') + ' — ' + branding.value.brandName}</title>
</svelte:head>

{#if loading}
    <Card.Root>
        <Card.Header class="h-48 animate-pulse bg-muted" />
    </Card.Root>
{:else if error}
    <Card.Root class="py-12 text-center">
        <Card.Content class="flex flex-col items-center gap-3 pt-6">
            <div class="flex size-10 items-center justify-center rounded-full bg-destructive/10">
                <CircleAlert class="size-5 text-destructive" aria-hidden="true" />
            </div>
            <h1 class="text-lg font-semibold">{error}</h1>
            <p class="text-sm text-muted-foreground">
                It may have been deleted, or the link is incorrect.
            </p>
            <Button variant="outline" size="sm" href={`/board/${slug}`} class="mt-2">
                ← Back to board
            </Button>
        </Card.Content>
    </Card.Root>
{:else if post}
    <Button variant="link" size="sm" href={`/board/${slug}`} class="max-sm:h-11 px-0 text-muted-foreground">
        ← Back to board
    </Button>

    <!-- Card.Header is a grid; data-slot="card-action" activates the built-in
         `grid-cols-[1fr_auto]` layout so the vote rail docks top-right (#feedback). -->
    <Card.Root class="mt-3">
        <Card.Header class="items-stretch gap-0">
            <div class="min-w-0" data-slot="card-action">
                <div class="mb-2 flex flex-wrap items-center gap-2">
                    <!-- Editors get one control per field (the select shows the
                         current value); everyone else sees the read-only badge. -->
                    {#if !canEditStatus}
                        <CategoryBadge category={post.category} />
                    {:else}
                        <select
                            value={post.category}
                            onchange={handleCategoryChange}
                            aria-label="Change category"
                            class="h-11 rounded-md border border-input bg-background px-2 text-base capitalize sm:h-7 sm:text-xs"
                        >
                            {#each categoryOptions as c (c)}
                                <option value={c} class="capitalize">{c}</option>
                            {/each}
                        </select>
                    {/if}
                    {#if !canEditStatus}
                        <StatusBadge status={post.status} />
                    {:else}
                        <select
                            value={post.status}
                            onchange={handleStatusChange}
                            aria-label="Change status"
                            class="h-11 rounded-md border border-input bg-background px-2 text-base capitalize sm:h-7 sm:text-xs"
                        >
                            {#each statusOptions as s (s)}
                                <option value={s} class="capitalize">{s.replace('_', ' ')}</option>
                            {/each}
                        </select>
                    {/if}
                </div>
                <h1 class="text-2xl leading-tight font-semibold tracking-tight">{post.title}</h1>
                <div class="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                    <span>{post.creator.name || 'User'}</span>
                    <span>·</span>
                    <span>{timeAgo(post.created_at)}</span>
                </div>
            </div>
            <div class="flex shrink-0 items-center border-l border-[var(--vote-rail-divider-color)] pl-4">
                <VoteRail postId={post.id} postTitle={post.title} voted={post.user_voted} count={post.vote_count} onvote={detail.applyVote} />
            </div>
        </Card.Header>
        {#if post.description}
            <Card.Content>
                <div class="whitespace-pre-wrap text-sm leading-relaxed text-foreground">
                    {post.description}
                </div>
            </Card.Content>
        {/if}
    </Card.Root>

    <!-- Attachments belong to the post body, so they sit directly under it
         (they used to float between the team response and the comments). -->
    <section class="mt-4" aria-label="Attachments">
        <AttachmentGallery postId={post.id} canEdit={detail.canModerate} />
    </section>

    <!-- Inline similar posts (#206): visible dedup between body and comments. -->
    {#if similar.length > 0}
        <section class="mt-4" aria-label="Similar posts">
            <Card.Root class="p-4">
                <h2 class="mb-2 text-sm font-semibold">Similar posts</h2>
                <ul class="flex flex-col gap-1">
                    {#each similar as s (s.id)}
                        <li>
                            <a
                                href={`/board/${slug}/post/${s.id}`}
                                class="flex min-h-11 items-center justify-between gap-3 rounded-md px-2 py-1.5 text-sm transition-colors hover:bg-muted"
                            >
                                <span class="min-w-0 truncate" title={s.title}>{s.title}</span>
                                <span class="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                                    <ChevronUp class="size-3.5" aria-hidden="true" />
                                    {s.vote_count}
                                </span>
                            </a>
                        </li>
                    {/each}
                </ul>
            </Card.Root>
        </section>
    {/if}

    <!-- Official team response (#205): pinned above the thread, distinct tint
         + left accent border, role badge. -->
    {#if official}
        <section class="mt-4" aria-label="Official team response">
            <div class="rounded-xl border border-primary/30 border-l-4 border-l-primary bg-primary/5 p-4">
                <div class="mb-2 flex flex-wrap items-center gap-2">
                    <span class="inline-flex items-center gap-1 rounded-full bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground">
                        <ShieldCheck class="size-3.5" aria-hidden="true" /> Team
                    </span>
                    <span class="text-sm font-semibold text-foreground">{official.creator?.name || 'Team'}</span>
                    <span class="text-xs text-muted-foreground">· {timeAgo(official.created_at)}</span>
                    {#if isAdmin}
                        <Button
                            variant="ghost"
                            size="xs"
                            class="ml-auto"
                            disabled={officialBusy}
                            onclick={() => detail.setOfficialResponse(null)}
                        >
                            Unpin response
                        </Button>
                    {/if}
                </div>
                <p class="whitespace-pre-wrap text-sm leading-relaxed text-foreground">{official.content}</p>
            </div>
        </section>
    {/if}

    <section class="mt-6">
        <h2 class="mb-4 text-sm font-semibold">Comments ({comments.length})</h2>

        {#if isAdmin && !official}
            <div class="mb-4 rounded-xl border border-dashed border-border p-3 text-sm text-muted-foreground">
                No official response yet. Pin one of the comments below as the team's answer.
            </div>
        {/if}

        {#if user}
            <form onsubmit={handleComment} class="mb-6">
                {#if replyTo}
                    {@const parentComment = comments.find((c) => c.id === replyTo)}
                    {#if parentComment}
                        <div class="mb-2 rounded-lg border border-border bg-muted/50 p-3">
                            <div class="flex items-center gap-2 text-xs text-muted-foreground">
                                <span class="font-medium">{parentComment.creator?.name || 'User'}</span>
                                <span>·</span>
                                <span>{timeAgo(parentComment.created_at)}</span>
                            </div>
                            <p class="mt-1 line-clamp-2 text-xs text-muted-foreground" title={parentComment.content}>{parentComment.content}</p>
                        </div>
                    {/if}
                    <div class="mb-1 text-xs text-muted-foreground">
                        Replying to thread ·
                        <button type="button" onclick={() => detail.cancelReply()} class="text-primary-text">cancel</button>
                    </div>
                {/if}
                <Textarea
                    id="post-reply"
                    bind:value={detail.commentText}
                    placeholder={replyTo ? 'Write a reply...' : 'Share your thoughts...'}
                    rows={3}
                />
                <div class="mt-2 flex justify-end">
                    <Button type="submit" disabled={!detail.commentText.trim() || commentLoading}>
                        {commentLoading ? 'Posting...' : 'Comment'}
                    </Button>
                </div>
            </form>
        {:else}
            <Card.Root class="mb-6">
                <Card.Content class="pt-6 text-center text-sm text-muted-foreground">
                    <a href={loginHref(page.url.pathname)} class="font-medium text-primary-text hover:underline">Login</a> to join the discussion
                </Card.Content>
            </Card.Root>
        {/if}

        <CommentThread
            {comments}
            currentUserId={user?.id}
            pinnedId={official?.id}
            onreply={user ? startReply : undefined}
            ondelete={(id) => detail.deleteComment(id)}
        >
            {#snippet commentActions(comment: Comment)}
                {#if isAdmin && official?.id !== comment.id}
                    <Button
                        variant="ghost"
                        size="xs"
                        disabled={officialBusy}
                        onclick={() => detail.setOfficialResponse(comment.id)}
                        title="Pin this comment as the official team response"
                    >
                        <ShieldCheck class="size-3.5" aria-hidden="true" /> Official response
                    </Button>
                {/if}
            {/snippet}
        </CommentThread>
    </section>
{/if}
