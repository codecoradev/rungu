<script lang="ts">
    import type { Comment } from '$lib/api/types';
    import { timeAgo } from '$lib/utils';
    import { Button } from '$lib/components/ui/button';

    let {
        comments,
        currentUserId,
        onreply,
        ondelete,
    }: {
        comments: Comment[];
        currentUserId?: string;
        onreply?: (parentId: string) => void;
        ondelete?: (id: string) => void;
    } = $props();

    // Build parent → children map
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

    function avatarChar(c: Comment): string {
        return (c.creator?.name || c.creator?.email || c.created_by)[0]?.toUpperCase() ?? '?';
    }

    function displayName(c: Comment): string {
        return c.creator?.name || c.creator?.email || 'User';
    }
</script>

{#snippet replyTree(parentId: string | null, depth: number)}
    {@const replies = getReplies(parentId)}
    {#each replies as comment (comment.id)}
        <div
            class={depth === 0
                ? 'rounded-lg border border-border bg-card p-4'
                : 'rounded-lg bg-muted p-3'}
        >
            <div class="flex items-center gap-2 text-sm">
                <div
                    class={depth === 0
                        ? 'flex size-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground'
                        : 'flex size-6 items-center justify-center rounded-full bg-background text-xs font-medium text-muted-foreground'}
                >
                    {avatarChar(comment)}
                </div>
                <span class="font-medium">{displayName(comment)}</span>
                <span class="text-muted-foreground">· {timeAgo(comment.created_at)}</span>
            </div>
            <p class="mt-1.5 whitespace-pre-wrap text-sm text-muted-foreground">{comment.content}</p>
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

            {#if getReplies(comment.id).length > 0}
                <div class="mt-3 ml-10 space-y-3 border-l-2 border-border pl-4">
                    {@render replyTree(comment.id, depth + 1)}
                </div>
            {/if}
        </div>
    {/each}
{/snippet}

<div class="space-y-4">
    {@render replyTree(null, 0)}
</div>

{#if comments.length === 0}
    <p class="py-8 text-center text-sm text-muted-foreground">No comments yet. Be the first to share!</p>
{/if}
