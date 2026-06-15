<script lang="ts">
    import { api } from '$lib/api/client';
    import type { ProviderInfo } from '$lib/api/types';
    import { onMount } from 'svelte';

    let providers: ProviderInfo[] = $state([]);

    onMount(async () => {
        try {
            const result = await api.getProviders();
            providers = result.providers;
        } catch {
            providers = [];
        }
    });

    const providerMeta: Record<string, { label: string; icon: string }> = {
        google: { label: 'Google', icon: '🔍' },
        github: { label: 'GitHub', icon: '🐙' },
        keycloak: { label: 'Keycloak', icon: '🔑' },
    };
</script>

<div class="flex flex-col gap-3">
    {#each providers as provider (provider.name)}
        {@const meta = providerMeta[provider.name] ?? { label: provider.name, icon: '🔑' }}
        <button
            onclick={() => api.login(provider.name)}
            class="flex items-center justify-center gap-2 rounded-lg border border-gray-200 bg-white px-4 py-2.5 font-medium text-gray-700 transition-colors hover:bg-gray-50"
        >
            <span class="text-lg">{meta.icon}</span>
            <span>Continue with {meta.label}</span>
        </button>
    {/each}

    {#if providers.length === 0}
        <p class="text-center text-sm text-gray-400">No auth providers configured.</p>
    {/if}
</div>
