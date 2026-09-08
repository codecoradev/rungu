<script lang="ts">
    import type { PostDetail } from '$lib/api/types';
    import StatusBadge from './StatusBadge.svelte';
    import CategoryBadge from './CategoryBadge.svelte';
    import VoteRail from './VoteRail.svelte';
    import MessageSquare from '@lucide/svelte/icons/message-square';
    import { timeAgo } from '$lib/utils';

    let { post, slug }: { post: PostDetail; slug: string } = $props();
</script>

<!--
    Flat hairline row (#201): no card chrome, border-b divider, token-driven
    density (--board-row-*) so the list fits ~5 rows per desktop viewport.
    Focus ring draws inside the row to avoid double-divider artifacts.
-->
<a
    href={`/board/${slug}/post/${post.id}`}
    class="group flex min-h-[var(--board-row-min-height)] items-stretch gap-3 rounded-lg border-b border-[var(--board-row-divider-color)] px-[var(--board-row-pad-x)] py-[var(--board-row-pad-y)] outline-none transition-colors last:border-b-0 hover:bg-[var(--board-row-hover-bg)] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
>
    <div class="min-w-0 flex-1">
        <div class="mb-1 flex items-center gap-2">
            <CategoryBadge category={post.category} />
            <StatusBadge status={post.status} />
        </div>
        <h3 class="truncate text-[length:var(--board-title-size)] font-semibold" title={post.title}>{post.title}</h3>
        {#if post.description}
            <p class="mt-1 line-clamp-2 text-[length:var(--board-body-size)] leading-snug text-muted-foreground" title={post.description}>{post.description}</p>
        {/if}
        <div class="mt-1.5 flex items-center gap-3 text-[length:var(--board-meta-size)] text-muted-foreground">
            <span>{post.creator.name || 'User'}</span>
            <span aria-hidden="true">·</span>
            <span class="whitespace-nowrap">{timeAgo(post.created_at)}</span>
            {#if post.comment_count > 0}
                <span class="flex items-center gap-1 whitespace-nowrap">
                    <MessageSquare class="size-3.5" aria-hidden="true" />
                    {post.comment_count}
                </span>
            {/if}
        </div>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events,a11y_no_static_element_interactions -->
    <div
        class="flex shrink-0 items-center border-l border-[var(--vote-rail-divider-color)] pl-2"
        onclick={(e) => e.preventDefault()}
    >
        <VoteRail postId={post.id} voted={post.user_voted} count={post.vote_count} />
    </div>
</a>
