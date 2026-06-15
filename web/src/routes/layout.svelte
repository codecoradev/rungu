<script lang="ts">
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { api } from '$lib/api/api';
    import type { CurrentUser } from '$lib/api/types';

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
    <!-- Navbar -->
    <nav class="border-b border-gray-200 bg-white">
        <div class="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
            <a href="/" class="flex items-center gap-2 font-bold text-gray-900">
                <span class="text-xl">🛡️</span>
                <span>Rungu</span>
            </a>

            <div class="flex items-center gap-4">
                {#if loading}
                    <div class="size-8 animate-pulse rounded-full bg-gray-100"></div>
                {:else if user}
                    <span class="text-sm text-gray-500">{user.email}</span>
                    <button
                        onclick={handleLogout}
                        class="rounded-lg px-3 py-1.5 text-sm font-medium text-gray-600 hover:bg-gray-50"
                    >
                        Logout
                    </button>
                {:else}
                    <a
                        href="/login"
                        class="rounded-lg bg-brand-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-brand-700"
                    >
                        Login
                    </a>
                {/if}
            </div>
        </div>
    </nav>

    <!-- Page content -->
    <main class="mx-auto max-w-5xl px-4 py-6">
        {@render children()}
    </main>

    <!-- Footer -->
    <footer class="border-t border-gray-200 py-4 text-center text-xs text-gray-400">
        <p>Rungu · Lightweight Feedback Board · Apache-2.0</p>
    </footer>
</div>
