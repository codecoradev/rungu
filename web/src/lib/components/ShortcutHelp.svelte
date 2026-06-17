<script lang="ts">
    /**
     * Keyboard-shortcut help overlay. Auto-generated from the shortcut registry
     * so the visible help can never drift from the actual bindings.
     *
     * Triggered globally by `?` (see +layout.svelte). Closes on `Esc`,
     * outside-click, or the close button.
     */
    import { shortcutsByScope, type ShortcutDef } from '$lib/shortcuts';
    import { Button } from '$lib/components/ui/button';

    let { open = $bindable(false) }: { open: boolean } = $props();

    const groups = shortcutsByScope();
    const SCOPE_LABELS: Record<keyof typeof groups, string> = {
        global: 'Global',
        board: 'Board',
        'post-detail': 'Post detail',
    };

    function close() {
        open = false;
    }

    function onWindowClick(e: MouseEvent) {
        // Close on click outside the dialog panel.
        const target = e.target as HTMLElement | null;
        if (target?.closest('[data-shortcut-help-panel]')) return;
        close();
    }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => e.key === 'Escape' && close()} />

{#if open}
    <!-- svelte-ignore a11y_click_events_have_keyevents, a11y_no_static_element_interactions -->
    <div
        class="fixed inset-0 z-50 flex items-start justify-center bg-background/80 p-4 backdrop-blur-sm sm:pt-20"
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
    >
        <div
            data-shortcut-help-panel
            class="w-full max-w-lg rounded-xl border border-border bg-card shadow-lg"
        >
            <div class="flex items-center justify-between border-b border-border px-5 py-3">
                <h2 class="text-sm font-semibold">Keyboard shortcuts</h2>
                <Button variant="ghost" size="icon-sm" onclick={close} aria-label="Close">
                    ✕
                </Button>
            </div>

            <div class="divide-y divide-border">
                {#each Object.entries(groups) as [scope, defs] (scope)}
                    {#if defs.length > 0}
                        <div class="px-5 py-3">
                            <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                                {SCOPE_LABELS[scope as keyof typeof SCOPE_LABELS] ?? scope}
                            </h3>
                            <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
                                {#each defs as def (def.key + def.scope)}
                                    <dt>
                                        <kbd
                                            class="inline-flex min-w-[1.5rem] items-center justify-center rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-xs"
                                        >
                                            {def.key}
                                        </kbd>
                                    </dt>
                                    <dd class="flex items-center gap-2">
                                        <span>{def.label}</span>
                                        {#if def.authRequired}
                                            <span class="text-xs text-muted-foreground">(login required)</span>
                                        {/if}
                                        {#if def.hint}
                                            <span class="text-xs text-muted-foreground">— {def.hint}</span>
                                        {/if}
                                    </dd>
                                {/each}
                            </dl>
                        </div>
                    {/if}
                {/each}
            </div>

            <div class="border-t border-border px-5 py-3 text-xs text-muted-foreground">
                Shortcuts are disabled while typing. Every action is also available via the visible UI.
            </div>
        </div>
    </div>
{/if}
