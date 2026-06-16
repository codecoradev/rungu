<script lang="ts">
    import { onMount } from 'svelte';
    import { api } from '$lib/api/client';
    import AuthProviderButtons from '$lib/components/AuthProviderButtons.svelte';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';

    let checking = $state(true);

    onMount(async () => {
        try {
            await api.getCurrentUser();
            window.location.href = '/';
        } catch {
            checking = false;
        }
    });
</script>

<svelte:head>
    <title>Login — Rungu</title>
</svelte:head>

<div class="mx-auto max-w-sm py-16">
    {#if checking}
        <Card.Root>
            <Card.Header>
                <Skeleton class="h-8 w-3/4" />
                <Skeleton class="mt-2 h-4 w-full" />
            </Card.Header>
            <Card.Content>
                <Skeleton class="h-10 w-full" />
            </Card.Content>
        </Card.Root>
    {:else}
        <Card.Root>
            <Card.Header class="text-center">
                <Card.Title class="text-2xl">Welcome back</Card.Title>
                <Card.Description>Sign in to share your feedback</Card.Description>
            </Card.Header>
            <Card.Content>
                <AuthProviderButtons />
            </Card.Content>
        </Card.Root>
    {/if}
</div>
