<script lang="ts">
    import '../app.css';
    import { onMount } from 'svelte';
    import { api } from '$lib/api/client';
    import type { CurrentUser } from '$lib/api/types';
    import { Button } from '$lib/components/ui/button';

    let { children } = $props();
    let user = $state<CurrentUser | null>(null);
    let loading = $state(true);

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
</script>

<div class="min-h-screen">
    <nav class="border-b border-border bg-background">
        <div class="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
            <a href="/" class="flex items-center gap-2 font-bold">
                <span class="text-xl">🛡️</span>
                <span>Rungu</span>
            </a>

            <div class="flex items-center gap-2">
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

    <main class="mx-auto max-w-5xl px-4 py-6">
        {@render children()}
    </main>

    <footer class="border-t border-border py-4 text-center text-xs text-muted-foreground">
        <p>Rungu · Lightweight Feedback Board · Apache-2.0</p>
    </footer>
</div>
