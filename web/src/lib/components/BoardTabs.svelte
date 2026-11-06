<script lang="ts">
    import { cn } from '$lib/utils';

    let { slug, current }: { slug: string; current: 'feedback' | 'roadmap' | 'changelog' } = $props();

    const views = [
        { id: 'feedback', label: 'Feedback', path: '' },
        { id: 'roadmap', label: 'Roadmap', path: '/roadmap' },
        { id: 'changelog', label: 'Changelog', path: '/changelog' },
    ] as const;
</script>

<!--
    Board view navigation, shared by the Feedback, Roadmap and Changelog pages
    so every surface of a board has the same way to switch views. These are
    page links, not ARIA tabs: the active one carries aria-current="page".
-->
<nav aria-label="Board views" class="mb-4 border-b">
    <div class="flex gap-1 overflow-x-auto">
        {#each views as view (view.id)}
            <a
                href={`/board/${slug}${view.path}`}
                aria-current={current === view.id ? 'page' : undefined}
                class={cn(
                    'whitespace-nowrap border-b-2 px-3 py-2 text-sm transition-colors max-sm:inline-flex max-sm:h-11 max-sm:items-center',
                    current === view.id
                        ? 'border-primary font-medium text-foreground'
                        : 'border-transparent text-muted-foreground hover:text-foreground',
                )}
            >
                {view.label}
            </a>
        {/each}
    </div>
</nav>
