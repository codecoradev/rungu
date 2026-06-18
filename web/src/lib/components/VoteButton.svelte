<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import { api, ApiError } from '$lib/api/client';
    import { cn } from '$lib/utils';

    let {
        postId,
        voted = false,
        count = 0,
        disabled = false,
        onvote,
    }: {
        postId: string;
        voted?: boolean;
        count?: number;
        disabled?: boolean;
        onvote?: (voted: boolean, count: number) => void;
    } = $props();

    let loading = $state(false);
    let error = $state('');

    async function toggle() {
        if (disabled || loading) return;

        // Optimistic update — revert on failure
        const prevVoted = voted;
        const prevCount = count;
        voted = !voted;
        count = voted ? count + 1 : count - 1;
        onvote?.(voted, count);

        loading = true;
        error = '';
        try {
            const result = await api.toggleVote(postId);
            // Reconcile with server response
            voted = result.voted;
            count = result.vote_count;
            onvote?.(result.voted, result.vote_count);
        } catch (e) {
            // Revert optimistic update
            voted = prevVoted;
            count = prevCount;
            onvote?.(prevVoted, prevCount);
            error = e instanceof ApiError && e.status === 401 ? 'Login to vote' : 'Failed to vote';
        } finally {
            loading = false;
        }
    }
</script>

<Button
    variant={voted ? 'default' : 'outline'}
    size="sm"
    {disabled}
    onclick={toggle}
    class={cn('gap-1.5 transition-opacity', loading && 'opacity-50')}
    title={error || undefined}
>
    {#if error}
        <span class="text-destructive text-xs">{error}</span>
    {/if}
    <svg
        class="size-4"
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 20 20"
        fill={voted ? 'currentColor' : 'none'}
        stroke="currentColor"
        stroke-width="1.5"
    >
        <path d="M10 3l2.5 5 5.5.8-4 3.9.9 5.5L10 16l-4.9 2.6.9-5.5-4-3.9 5.5-.8L10 3z" stroke-linejoin="round" />
    </svg>
    <span>{count}</span>
</Button>
