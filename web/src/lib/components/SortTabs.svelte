<script lang="ts">
    import { cn } from '$lib/utils';

    let {
        options,
        value,
        onchange,
    }: {
        options: { value: string; label: string }[];
        value: string;
        onchange: (value: string) => void;
    } = $props();
</script>

<!--
    Segmented sort tabs (#203): obvious selected state driven by #199 tokens
    (--sort-tab-*). role="tablist"/"tab" with aria-selected; height bumps to
    the 44px touch minimum below sm.
-->
<div role="tablist" aria-label="Sort posts" class="flex items-center gap-1 overflow-x-auto">
    {#each options as opt (opt.value)}
        <button
            type="button"
            role="tab"
            aria-selected={value === opt.value}
            onclick={() => onchange(opt.value)}
            class={cn(
                'flex min-h-[var(--vote-target-min)] shrink-0 items-center whitespace-nowrap rounded-[var(--sort-tab-radius)] px-3 sm:min-h-[var(--sort-tab-height)]',
                'text-sm font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring',
                value === opt.value
                    ? 'bg-[var(--sort-tab-active-bg)] text-[var(--sort-tab-active-fg)]'
                    : 'text-[var(--sort-tab-inactive-fg)] hover:bg-[var(--sort-tab-inactive-hover-bg)]',
            )}
        >
            {opt.label}
        </button>
    {/each}
</div>
