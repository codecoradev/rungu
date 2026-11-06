<script lang="ts">
    /**
     * Keyboard-shortcut help overlay. Auto-generated from the shortcut registry
     * so the visible help can never drift from the actual bindings.
     *
     * Triggered globally by `?` (see +layout.svelte). Built on the bits-ui
     * Dialog: focus moves in and is trapped, Esc / outside-click / the close
     * button dismiss it, and focus returns to whatever opened it.
     */
    import { Dialog } from 'bits-ui';
    import { shortcutsByScope, type ShortcutDef } from '$lib/shortcuts';
    import { Button } from '$lib/components/ui/button';

    let { open = $bindable(false) }: { open: boolean } = $props();

    const groups = shortcutsByScope();
    const SCOPE_LABELS: Record<keyof typeof groups, string> = {
        global: 'Global',
        board: 'Board',
        'post-detail': 'Post detail',
    };
</script>

<Dialog.Root bind:open>
    <Dialog.Portal>
        <Dialog.Overlay class="fixed inset-0 z-50 bg-background/80 backdrop-blur-sm" />
        <Dialog.Content
            class="fixed inset-x-4 top-4 z-50 mx-auto max-h-[calc(100vh-2rem)] max-w-lg overflow-y-auto rounded-xl border border-border bg-card shadow-lg outline-none sm:top-20"
        >
            <div class="flex items-center justify-between border-b border-border px-5 py-3">
                <Dialog.Title class="text-sm font-semibold">Keyboard shortcuts</Dialog.Title>
                <Dialog.Close>
                    {#snippet child({ props })}
                        <Button {...props} variant="ghost" size="icon-sm" aria-label="Close">✕</Button>
                    {/snippet}
                </Dialog.Close>
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
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
