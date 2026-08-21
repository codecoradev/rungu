<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import { api } from '$lib/api/client';
    import type { ProviderInfo } from '$lib/api/types';
    import { onMount } from 'svelte';
    import { KeyRound } from '@lucide/svelte';

    let { redirectTo = '/' }: { redirectTo?: string } = $props();

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

    function handleLogin(provider: string) {
        // Thread ?redirect= through the OAuth flow so users land back where
        // they started (e.g. the post they tried to vote on). #147
        const url = new URL(`/auth/${provider}/login`, window.location.origin);
        url.searchParams.set('redirect', redirectTo);
        window.location.href = url.pathname + url.search;
    }

    const providerMeta: Record<string, { label: string }> = {
        google: { label: 'Google' },
        github: { label: 'GitHub' },
        keycloak: { label: 'Keycloak' },
    };
</script>

<div class="flex flex-col gap-3">
    {#each providers as provider (provider.name)}
        {@const meta = providerMeta[provider.name] ?? { label: provider.name }}
        <Button
            variant="outline"
            size="lg"
            onclick={() => handleLogin(provider.name)}
            class="justify-start gap-2"
        >
            {#if provider.name === 'google'}
                <!-- Official Google "G" mark (multi-color, per brand guidelines) -->
                <svg class="size-5" viewBox="0 0 24 24" aria-hidden="true">
                    <path fill="#4285F4" d="M23.5 12.3c0-.9-.1-1.5-.3-2.2H12v4.3h6.5c-.1 1.1-.8 2.7-2.4 3.8l3.7 2.9c2.3-2.1 3.7-5.2 3.7-8.8z" />
                    <path fill="#34A853" d="M12 24c3.2 0 6-1.1 8-2.9l-3.7-2.9c-1 .7-2.4 1.2-4.3 1.2-3.3 0-6.1-2.2-7.1-5.2l-3.9 3C3 21.3 7.2 24 12 24z" />
                    <path fill="#FBBC05" d="M4.9 14.2c-.2-.7-.4-1.5-.4-2.2s.1-1.5.4-2.2l-3.9-3C.4 8.2 0 10 0 12s.4 3.8 1 5.2l3.9-3z" />
                    <path fill="#EA4335" d="M12 4.7c2.3 0 3.8 1 4.7 1.8l3.3-3.2C18 1.6 15.2 0 12 0 7.2 0 3 2.7 1 6.7l3.9 3c1-3 3.8-5 7.1-5z" />
                </svg>
            {:else if provider.name === 'github'}
                <svg class="size-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                    <path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.27-.01-1.17-.02-2.12-3.2.7-3.88-1.36-3.88-1.36-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.72-1.54-2.55-.29-5.24-1.28-5.24-5.68 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .96-.31 3.16 1.18a11 11 0 0 1 5.76 0c2.2-1.49 3.16-1.18 3.16-1.18.62 1.59.23 2.76.11 3.05.74.81 1.18 1.83 1.18 3.09 0 4.41-2.69 5.38-5.25 5.67.41.35.77 1.04.77 2.1 0 1.52-.01 2.74-.01 3.11 0 .31.21.68.8.56A11.51 11.51 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5z" />
                </svg>
            {:else}
                <KeyRound class="size-5" aria-hidden="true" />
            {/if}
            <span>Continue with {meta.label}</span>
        </Button>
    {/each}

    {#if providers.length === 0}
        <div class="rounded-lg border border-dashed border-border p-4 text-center">
            {#if loadError}
                <p class="text-sm text-muted-foreground">Failed to load auth providers. Check your connection and refresh.</p>
            {:else}
                <p class="text-sm font-medium">No login methods configured</p>
                <p class="mt-1 text-sm text-muted-foreground">
                    This Rungu instance has no OAuth provider enabled. Admins can enable Google, GitHub or Keycloak in the server configuration.
                </p>
            {/if}
        </div>
    {/if}
</div>
