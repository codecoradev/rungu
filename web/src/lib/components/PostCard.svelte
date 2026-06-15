<script lang="ts">
    import type { PostDetail } from '$lib/api/types';
    import StatusBadge from './StatusBadge.svelte';
    import CategoryBadge from './CategoryBadge.svelte';
    import VoteButton from './VoteButton.svelte';
    import { timeAgo } from '$lib/utils';
    import * as Card from '$lib/components/ui/card';

    let { post, slug }: { post: PostDetail; slug: string } = $props();
</script>

<a href={`/board/${slug}/post/${post.id}`} class="block">
    <Card.Root class="transition-shadow hover:shadow-md">
        <Card.Header class="flex-row items-start gap-3">
            <div class="shrink-0" onclick={(e) => e.preventDefault()}>
                <VoteButton postId={post.id} voted={post.user_voted} count={post.vote_count} />
            </div>
            <div class="min-w-0 flex-1">
                <div class="mb-1 flex items-center gap-2">
                    <CategoryBadge category={post.category} />
                    <StatusBadge status={post.status} />
                </div>
                <Card.Title class="truncate text-base">{post.title}</Card.Title>
                {#if post.description}
                    <p class="mt-1 line-clamp-2 text-sm text-muted-foreground">{post.description}</p>
                {/if}
                <div class="mt-2 flex items-center gap-3 text-xs text-muted-foreground">
                    <span>{post.creator.name || post.creator.email}</span>
                    <span>·</span>
                    <span>{timeAgo(post.created_at)}</span>
                    {#if post.comment_count > 0}
                        <span>·</span>
                        <span class="flex items-center gap-1">
                            <svg class="size-3.5" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
                                <path d="M2 4a2 2 0 012-2h12a2 2 0 012 2v8a2 2 0 01-2 2H7l-5 4V4z" />
                            </svg>
                            {post.comment_count}
                        </span>
                    {/if}
                </div>
            </div>
        </Card.Header>
    </Card.Root>
</a>
