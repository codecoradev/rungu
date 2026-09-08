<script lang="ts">
    import { api, ApiError } from '$lib/api/client';
    import { toastError } from '$lib/toast.svelte';
    import { cn } from '$lib/utils';
    import ChevronUp from '@lucide/svelte/icons/chevron-up';

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

        // Optimistic update — snapshot prev state, revert on failure (#174 pattern)
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
                action: isAuth
                    ? { label: 'Login →', href: `/login?redirect=${encodeURIComponent(location.pathname)}` }
                    : undefined,
            });
        } finally {
            loading = false;
        }
    }
</script>

<!--
    Vertical vote rail (#200): chevron stacked above the count, full-height
    hit target (>= 44px, --vote-target-min), token-driven states (#199).
    Divider is drawn by the parent (border on the rail wrapper).
-->
<button
    type="button"
    onclick={toggle}
    disabled={disabled}
    aria-pressed={voted}
    aria-label={voted ? 'Remove vote' : 'Vote'}
    class={cn(
        'group flex w-[var(--vote-rail-width)] shrink-0 flex-col items-center justify-center gap-0.5 rounded-lg border outline-none transition-colors',
        'min-h-[var(--vote-target-min)]',
        'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background',
        voted
            ? 'border-[var(--vote-voted-border)] bg-[var(--vote-voted-bg)] text-[var(--vote-voted-fg)]'
            : 'border-transparent text-muted-foreground hover:bg-[var(--vote-hover-bg)]',
        'active:translate-y-px',
        loading && 'opacity-50',
    )}
>
    <ChevronUp
        class="size-5 transition-transform duration-150 group-hover:-translate-y-0.5"
        strokeWidth={2}
        aria-hidden="true"
    />
    <span class="text-[length:var(--vote-count-size)] leading-none font-semibold tabular-nums">{count}</span>
</button>
