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
    let errorIsAuth = $state(false);
    let errorTimer: ReturnType<typeof setTimeout> | null = null;

    function showError(msg: string, isAuth: boolean) {
        error = msg;
        errorIsAuth = isAuth;
        if (errorTimer) clearTimeout(errorTimer);
        // Auto-dismiss: transient feedback, no persistent layout shift.
        errorTimer = setTimeout(() => (error = ''), 4000);
    }

    // Clear the pending error timer when the component unmounts.
    $effect(() => {
        return () => {
            if (errorTimer) clearTimeout(errorTimer);
        };
    });

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
            showError(
                e instanceof ApiError && e.status === 401 ? 'Login to vote' : 'Failed to vote',
                e instanceof ApiError && e.status === 401,
            );
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

{#if error}
    <!-- Floating toast: feedback without shifting the button or card layout.
         Fixed-position overlay, auto-dismisses after 4s (see showError). -->
    <div
        role="status"
        class="fixed bottom-4 left-1/2 z-50 -translate-x-1/2 rounded-lg border border-destructive/40 bg-background px-4 py-2 text-sm shadow-lg"
    >
        {error}
        {#if errorIsAuth}
            <a href="/login" class="ml-2 font-medium text-primary hover:underline">Login →</a>
        {/if}
    </div>
{/if}
