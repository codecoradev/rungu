<script lang="ts">
    import '../app.css';
    import { onMount } from 'svelte';
    import { page } from '$app/state';
    import { api } from '$lib/api/client';
    import type { CurrentUser } from '$lib/api/types';
    import { Button } from '$lib/components/ui/button';
    import ThemeToggle from '$lib/components/ThemeToggle.svelte';
    import ShortcutHelp from '$lib/components/ShortcutHelp.svelte';
    import { resolveShortcut, type ShortcutDef } from '$lib/shortcuts';

    let { children } = $props();
    let user = $state<CurrentUser | null>(null);
    let loading = $state(true);
    let helpOpen = $state(false);

    onMount(async () => {
        try {
            user = await api.getCurrentUser();
        } catch {
            user = null;
        } finally {
            loading = false;
        }
    });

    async function handleLogout() {
        await api.logout();
        user = null;
        window.location.href = '/';
    }

    /**
     * Resolve the active shortcut scope(s) from the current URL.
     * - /board/{slug}            → ['global', 'board']
     * - /board/{slug}/post/{id}  → ['global', 'post-detail']
     * - everything else          → ['global']
     */
    function activeScopes(): ShortcutDef['scope'][] {
        const path = page.url.pathname;
        const scopes: ShortcutDef['scope'][] = ['global'];
        if (path.startsWith('/board/')) {
            if (path.includes('/post/')) {
                scopes.push('post-detail');
            } else {
                scopes.push('board');
            }
        }
        return scopes;
    }

    function onKeydown(e: KeyboardEvent) {
        const resolved = resolveShortcut(e, activeScopes());
        if (!resolved) return;
        const def = resolved.def;

        // Global handlers we own directly: open/close help, close on Esc.
        if (def.scope === 'global') {
            if (def.key === '?') {
                e.preventDefault();
                helpOpen = !helpOpen;
                return;
            }
            if (def.key === 'Esc' && helpOpen) {
                e.preventDefault();
                helpOpen = false;
                return;
            }
            // Esc with no modal open — let the browser handle blur natively.
            return;
        }

        // Scope-specific shortcuts are handled by the page via a custom event.
        // This keeps layout out of per-page DOM details (which post is focused, etc).
        window.dispatchEvent(
            new CustomEvent('rungu:shortcut', {
                detail: { key: def.key, scope: def.scope, authRequired: def.authRequired },
            }),
        );
        e.preventDefault();
    }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex min-h-screen flex-col">
    <nav class="border-b border-border bg-background">
        <div class="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
            <a href="/" class="flex items-center gap-2 font-bold">
                <span class="text-xl">🛡️</span>
                <span>Rungu</span>
            </a>

            <div class="flex items-center gap-2">
                <Button
                    variant="ghost"
                    size="icon-sm"
                    onclick={() => (helpOpen = true)}
                    aria-label="Keyboard shortcuts"
                    title="Keyboard shortcuts (?)"
                >
                    ?
                </Button>
                <ThemeToggle />
                {#if loading}
                    <div class="size-8 animate-pulse rounded-full bg-muted"></div>
                {:else if user}
                    <span class="text-sm text-muted-foreground">{user.email}</span>
                    {#if user.role === 'admin'}
                        <Button variant="ghost" size="sm" href="/admin">Admin</Button>
                    {/if}
                    <Button variant="outline" size="sm" onclick={handleLogout}>Logout</Button>
                {:else}
                    <Button size="sm" href="/login">Login</Button>
                {/if}
            </div>
        </div>
    </nav>

    <main class="mx-auto w-full max-w-5xl flex-1 px-4 py-6">
        {@render children()}
    </main>

    <footer class="border-t border-border py-4 text-center text-xs text-muted-foreground">
        <p>Rungu · Lightweight Feedback Board · Apache-2.0</p>
    </footer>
</div>

<ShortcutHelp bind:open={helpOpen} />
