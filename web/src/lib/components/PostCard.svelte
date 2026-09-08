<script lang="ts">
    import type { PostDetail } from '$lib/api/types';
    import StatusBadge from './StatusBadge.svelte';
    import CategoryBadge from './CategoryBadge.svelte';
    import VoteRail from './VoteRail.svelte';
    import MessageSquare from '@lucide/svelte/icons/message-square';
    import { timeAgo } from '$lib/utils';
    import * as Card from '$lib/components/ui/card';

    let { post, slug }: { post: PostDetail; slug: string } = $props();
</script>

<a href={`/board/${slug}/post/${post.id}`} class="block rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background">
    <!-- Card.Header is a grid: the data-slot="card-action" child activates the
         built-in `grid-cols-[1fr_auto]` two-column layout, putting the vote
         rail on the right edge. items-stretch lets the rail span full height. -->
    <Card.Root class="transition-shadow hover:shadow-md">
        <Card.Header class="items-stretch">
            <div class="min-w-0">
                <div class="mb-1 flex items-center gap-2">
                    <CategoryBadge category={post.category} />
                    <StatusBadge status={post.status} />
                </div>
                <Card.Title class="truncate text-base" title={post.title}>{post.title}</Card.Title>
                {#if post.description}
                    <p class="mt-1 line-clamp-2 text-sm text-muted-foreground" title={post.description}>{post.description}</p>
                {/if}
                <div class="mt-2 flex items-center gap-3 text-xs text-muted-foreground">
                    <span>{post.creator.name || 'User'}</span>
                    <span>·</span>
                    <span>{timeAgo(post.created_at)}</span>
                    {#if post.comment_count > 0}
                        <span>·</span>
                        <span class="flex items-center gap-1">
                            <MessageSquare class="size-3.5" aria-hidden="true" />
                            {post.comment_count}
                        </span>
                    {/if}
                </div>
            </div>
            <!-- svelte-ignore a11y_click_events_have_key_events,a11y_no_static_element_interactions -->
            <div
                data-slot="card-action"
                class="flex items-center border-l border-[var(--vote-rail-divider-color)] pl-2 ml-2 self-stretch"
                onclick={(e) => e.preventDefault()}
            >
                <VoteRail postId={post.id} voted={post.user_voted} count={post.vote_count} />
            </div>
        </Card.Header>
    </Card.Root>
</a>
