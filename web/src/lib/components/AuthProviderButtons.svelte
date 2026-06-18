<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import { api } from '$lib/api/client';
    import type { ProviderInfo } from '$lib/api/types';
    import { onMount } from 'svelte';

    let providers: ProviderInfo[] = $state([]);
    let loadError = $state(false);

    onMount(async () => {
        try {
            const result = await api.getProviders();
            providers = result.providers;
        } catch {
            loadError = true;
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
        <Button
            variant="outline"
            size="lg"
            onclick={() => api.login(provider.name)}
            class="justify-start gap-2"
        >
            <span class="text-lg">{meta.icon}</span>
            <span>Continue with {meta.label}</span>
        </Button>
    {/each}

    {#if providers.length === 0}
        <p class="text-center text-sm text-muted-foreground">
            {loadError ? 'Failed to load auth providers. Check your connection.' : 'No auth providers configured.'}
        </p>
    {/if}
</div>
