<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import Star from '@lucide/svelte/icons/star';
    import { api, ApiError } from '$lib/api/client';
    import { toastError } from '$lib/toast.svelte';
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

    async function toggle() {
        if (disabled || loading) return;

        // Optimistic update — revert on failure
        const prevVoted = voted;
        const prevCount = count;
        voted = !voted;
        count = voted ? count + 1 : count - 1;
        onvote?.(voted, count);

        loading = true;
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
            const isAuth = e instanceof ApiError && e.status === 401;
            toastError(isAuth ? 'Login to vote' : 'Failed to vote', {
                action: isAuth ? { label: 'Login →', href: `/login?redirect=${encodeURIComponent(location.pathname)}` } : undefined,
            });
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
    class={cn('min-h-11 gap-1.5 transition-opacity', loading && 'opacity-50')}
>
    <Star
        class="size-4"
        strokeWidth={1.5}
        fill={voted ? 'currentColor' : 'none'}
        aria-hidden="true"
    />
    <span>{count}</span>
</Button>
