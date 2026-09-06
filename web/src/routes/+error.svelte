<script lang="ts">
    import { branding } from '$lib/branding.svelte';
    import { page } from '$app/state';
    import CircleAlert from '@lucide/svelte/icons/circle-alert';
    import { Button } from '$lib/components/ui/button';
    import * as Card from '$lib/components/ui/card';

    // Routing-level errors (bad URL, render failure) land here. Without this
    // file SvelteKit renders its unstyled default error UI inside the layout.
    const message = $derived(
        page.status === 404
            ? "The page you're looking for doesn't exist."
            : (page.error?.message ?? 'Something went wrong.'),
    );
</script>

<svelte:head>
    <title>{page.status + ' — ' + branding.value.brandName}</title>
</svelte:head>

<Card.Root class="py-12 text-center">
    <Card.Content class="flex flex-col items-center gap-3 pt-6">
        <div class="flex size-10 items-center justify-center rounded-full bg-destructive/10">
            <CircleAlert class="size-5 text-destructive" aria-hidden="true" />
        </div>
        <h1 class="text-lg font-semibold">{page.status}</h1>
        <p class="text-sm text-muted-foreground">{message}</p>
        <Button variant="outline" size="sm" href="/" class="mt-2">← Back to all boards</Button>
    </Card.Content>
</Card.Root>
