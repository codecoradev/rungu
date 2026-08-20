<script lang="ts">
    import { onMount } from 'svelte';
    import { page } from '$app/state';
    import { api } from '$lib/api/client';
    import AuthProviderButtons from '$lib/components/AuthProviderButtons.svelte';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';

    let checking = $state(true);

    // Preserve where the user came from (#147): backend /auth/:provider/login
    // accepts ?redirect= (signed cookie, open-redirect validated) and sends
    // the user back there after successful auth. Client-side path must be
    // validated the same way — only same-origin absolute paths allowed.
    const rawRedirect = page.url.searchParams.get('redirect') || '/';
    const redirectTo =
        rawRedirect.startsWith('/') && !rawRedirect.startsWith('//') ? rawRedirect : '/';

    onMount(async () => {
        try {
            await api.getCurrentUser();
            window.location.href = redirectTo;
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
                <Card.Title class="text-2xl">Sign in to Rungu</Card.Title>
                <Card.Description>Sign in to share your feedback</Card.Description>
            </Card.Header>
            <Card.Content>
                <AuthProviderButtons {redirectTo} />
            </Card.Content>
        </Card.Root>
    {/if}
</div>
