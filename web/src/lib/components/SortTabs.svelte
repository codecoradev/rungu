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
    Sort control (#203). A row of toggle buttons (aria-pressed) from sm up,
    styled by the #199 --sort-tab-* tokens. They are not ARIA tabs: there is
    no tabpanel and no arrow-key model, so role="tab" promised behaviour the
    control never had. Below sm, four pills overflowed the row and cut off
    the last option, so phones get a native select instead.
-->
<div role="group" aria-label="Sort posts" class="hidden items-center gap-1 sm:flex">
    {#each options as opt (opt.value)}
        <button
            type="button"
            aria-pressed={value === opt.value}
            onclick={() => onchange(opt.value)}
            class={cn(
                'flex min-h-[var(--sort-tab-height)] shrink-0 items-center whitespace-nowrap rounded-[var(--sort-tab-radius)] px-3',
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

<label class="flex items-center gap-2 text-sm text-muted-foreground sm:hidden">
    <span>Sort</span>
    <select
        {value}
        onchange={(e) => onchange(e.currentTarget.value)}
        class="h-11 rounded-md border border-input bg-background px-3 text-base text-foreground"
    >
        {#each options as opt (opt.value)}
            <option value={opt.value}>{opt.label}</option>
        {/each}
    </select>
</label>
