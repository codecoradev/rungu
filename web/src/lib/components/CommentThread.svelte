<script lang="ts">
    import type { Comment } from '$lib/api/types';
    import { timeAgo } from '$lib/utils';
    import { Button } from '$lib/components/ui/button';

    let {
        comments,
        postId,
        currentUserId,
        onreply,
        ondelete,
    }: {
        comments: Comment[];
        postId: string;
        currentUserId?: string;
        onreply?: (parentId: string) => void;
        ondelete?: (id: string) => void;
    } = $props();

    const threads = $derived(buildThreads(comments));

    function buildThreads(items: Comment[]): Map<string, Comment[]> {
        const map = new Map<string, Comment[]>();
        for (const item of items) {
            const key = item.parent_id ?? 'root';
            if (!map.has(key)) map.set(key, []);
            map.get(key)!.push(item);
        }
        return map;
    }

    function getReplies(parentId: string | null): Comment[] {
        return threads.get(parentId ?? 'root') ?? [];
    }
</script>

<div class="space-y-4">
    {#each getReplies(null) as comment (comment.id)}
        {@const replies = getReplies(comment.id)}
        <div class="rounded-lg border border-border bg-card p-4">
            <div class="flex items-center gap-2 text-sm">
                <div class="flex size-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground">
                    {(comment.creator?.name || comment.creator?.email || comment.created_by)[0]?.toUpperCase()}
                </div>
                <span class="font-medium">{comment.creator?.name || comment.creator?.email || 'User'}</span>
                <span class="text-muted-foreground">· {timeAgo(comment.created_at)}</span>
            </div>
            <p class="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">{comment.content}</p>
            <div class="mt-2 flex gap-2">
                {#if onreply}
                    <Button variant="ghost" size="xs" onclick={() => onreply(comment.id)}>
                        Reply
                    </Button>
                {/if}
                {#if ondelete && comment.created_by === currentUserId}
                    <Button variant="ghost" size="xs" class="text-destructive" onclick={() => ondelete(comment.id)}>
                        Delete
                    </Button>
                {/if}
            </div>

            {#if replies.length > 0}
                <div class="mt-4 ml-10 space-y-3 border-l-2 border-border pl-4">
                    {#each replies as reply (reply.id)}
                        <div class="rounded-lg bg-muted p-3">
                            <div class="flex items-center gap-2 text-sm">
                                <div class="flex size-6 items-center justify-center rounded-full bg-background text-xs font-medium text-muted-foreground">
                                    {(reply.creator?.name || reply.creator?.email || reply.created_by)[0]?.toUpperCase()}
                                </div>
                                <span class="font-medium">{reply.creator?.name || reply.creator?.email || 'User'}</span>
                                <span class="text-muted-foreground">· {timeAgo(reply.created_at)}</span>
                            </div>
                            <p class="mt-1.5 whitespace-pre-wrap text-sm text-muted-foreground">{reply.content}</p>
                            {#if ondelete && reply.created_by === currentUserId}
                                <Button variant="ghost" size="xs" class="mt-1 text-destructive" onclick={() => ondelete(reply.id)}>
                                    Delete
                                </Button>
                            {/if}
                        </div>
                    {/each}
                </div>
            {/if}
        </div>
    {/each}
</div>

{#if comments.length === 0}
    <p class="py-8 text-center text-sm text-muted-foreground">No comments yet. Be the first to share!</p>
{/if}
