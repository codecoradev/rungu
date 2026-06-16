<script lang="ts">
    import { onMount } from 'svelte';
    import { api } from '$lib/api/client';
    import AuthProviderButtons from '$lib/components/AuthProviderButtons.svelte';

    let loggedIn = $state(false);
    let checking = $state(true);

    onMount(async () => {
        try {
            await api.getCurrentUser();
            // Already logged in — redirect to home
            window.location.href = '/';
        } catch {
            checking = false;
        }
    });
</script>

<svelte:head>
    <title>Login — Rungu</title>
</svelte:head>

{#if checking}
    <div class="py-16 text-center">
        <div class="inline-block size-8 animate-spin rounded-full border-2 border-muted border-t-primary"></div>
    </div>
{:else}
    <div class="mx-auto max-w-sm py-16">
        <div class="text-center">
            <h1 class="text-2xl font-bold">Welcome back</h1>
            <p class="mt-2 text-sm text-muted-foreground">Sign in to share your feedback</p>
        </div>

        <div class="mt-8">
            <AuthProviderButtons />
        </div>
    </div>
{/if}
