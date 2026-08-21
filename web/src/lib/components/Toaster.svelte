<script lang="ts">
    import { toasts, dismiss } from '$lib/toast.svelte';
    import { fade } from 'svelte/transition';

    // Single global toast viewport, bottom-center, stacked.
    // Error toasts: role=alert (assertive). Success: role=status (polite).
</script>

{#if $toasts.length > 0}
    <div class="fixed bottom-4 left-1/2 z-50 flex w-full max-w-sm -translate-x-1/2 flex-col gap-2 px-4">
        {#each $toasts as t (t.id)}
            <div
                role={t.kind === 'error' ? 'alert' : 'status'}
                transition:fade={{ duration: 150 }}
                class="flex items-center justify-between gap-3 rounded-lg border px-4 py-2.5 text-sm shadow-lg {t.kind === 'error'
                    ? 'border-destructive/40 bg-background text-foreground'
                    : 'border-success/40 bg-background text-foreground'}"
            >
                <span class="flex items-center gap-2">
                    <span
                        class="size-2 shrink-0 rounded-full {t.kind === 'error' ? 'bg-destructive' : 'bg-success'}"
                        aria-hidden="true"
                    ></span>
                    <span>{t.message}</span>
                    {#if t.action}
                        <a href={t.action.href} class="font-medium text-primary hover:underline">
                            {t.action.label}
                        </a>
                    {/if}
                </span>
                <button
                    type="button"
                    onclick={() => dismiss(t.id)}
                    aria-label="Dismiss notification"
                    class="shrink-0 text-muted-foreground hover:text-foreground"
                >
                    ✕
                </button>
            </div>
        {/each}
    </div>
{/if}
