<script lang="ts">
    import type { Comment } from '$lib/api/types';
    import { timeAgo } from '$lib/utils';

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

    // Build threaded structure
    const threads = $derived(buildThreads(comments));

    function buildThreads(items: Comment[]): Map<string | null, Comment[]> {
        const map = new Map<string | null, Comment[]>();
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
        <div class="rounded-lg border border-gray-100 bg-white p-4">
            <div class="flex items-center gap-2 text-sm">
                <div class="flex size-8 items-center justify-center rounded-full bg-gray-100 text-xs font-medium text-gray-500">
                    {(comment.creator.name || comment.creator.email)[0]?.toUpperCase()}
                </div>
                <span class="font-medium text-gray-700">
                    {comment.creator.name || comment.creator.email}
                </span>
                <span class="text-gray-400">· {timeAgo(comment.created_at)}</span>
            </div>
            <p class="mt-2 whitespace-pre-wrap text-sm text-gray-600">{comment.content}</p>
            <div class="mt-2 flex gap-3 text-xs">
                {#if onreply}
                    <button onclick={() => onreply(comment.id)} class="text-gray-400 hover:text-brand-600">
                        Reply
                    </button>
                {/if}
                {#if ondelete && comment.created_by === currentUserId}
                    <button onclick={() => ondelete(comment.id)} class="text-gray-400 hover:text-red-500">
                        Delete
                    </button>
                {/if}
            </div>

            {#if replies.length > 0}
                <div class="mt-4 ml-10 space-y-3 border-l-2 border-gray-50 pl-4">
                    {#each replies as reply (reply.id)}
                        <div class="rounded-lg bg-gray-50 p-3">
                            <div class="flex items-center gap-2 text-sm">
                                <div class="flex size-6 items-center justify-center rounded-full bg-white text-xs font-medium text-gray-500">
                                    {(reply.creator.name || reply.creator.email)[0]?.toUpperCase()}
                                </div>
                                <span class="font-medium text-gray-700">
                                    {reply.creator.name || reply.creator.email}
                                </span>
                                <span class="text-gray-400">· {timeAgo(reply.created_at)}</span>
                            </div>
                            <p class="mt-1.5 whitespace-pre-wrap text-sm text-gray-600">{reply.content}</p>
                            {#if ondelete && reply.created_by === currentUserId}
                                <button
                                    onclick={() => ondelete(reply.id)}
                                    class="mt-1 text-xs text-gray-400 hover:text-red-500"
                                >
                                    Delete
                                </button>
                            {/if}
                        </div>
                    {/each}
                </div>
            {/if}
        </div>
    {/each}
</div>

{#if comments.length === 0}
    <p class="py-8 text-center text-sm text-gray-400">No comments yet. Be the first to share!</p>
{/if}
