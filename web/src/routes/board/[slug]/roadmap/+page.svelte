<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, RoadmapResponse } from '$lib/api/types';
    import PostCard from '$lib/components/PostCard.svelte';
    import { Button } from '$lib/components/ui/button';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';

    let { params } = $props();
    let slug = $derived(params.slug);

    let project = $state<Project | null>(null);
    let roadmap = $state<RoadmapResponse | null>(null);
    let loading = $state(true);
    let error = $state('');

    // Per-bucket cap. Matches the server default of 10; bumping it re-fetches
    // with ?limit=N so users can expand a bucket beyond the first screen.
    let bucketLimit = $state(10);

    onMount(async () => {
        await Promise.all([loadProject(), loadRoadmap()]);
    });

    async function loadProject() {
        try {
            project = await api.getProject(slug);
        } catch {
            // Board may still render without the project header — the roadmap
            // data is what matters. Suppress; the 404 path is handled by loadRoadmap.
        }
    }

    async function loadRoadmap() {
        loading = true;
        error = '';
        try {
            roadmap = await api.getRoadmap(slug, bucketLimit);
        } catch (e) {
            if (e instanceof ApiError && e.status === 404) {
                error = 'Board not found.';
            } else {
                error = 'Failed to load roadmap. Please try again.';
            }
        } finally {
            loading = false;
        }
    }

    async function expandBuckets() {
        bucketLimit = Math.min(bucketLimit + 10, 50);
        await loadRoadmap();
    }

    // Column definition — order matches the status lifecycle (left → right).
    type BucketKey = 'planned' | 'in_progress' | 'done';
    const columns: { key: BucketKey; title: string; totalKey: keyof RoadmapResponse; accent: string }[] = [
        { key: 'planned', title: 'Planned', totalKey: 'planned_total', accent: 'border-t-blue-500' },
        { key: 'in_progress', title: 'In Progress', totalKey: 'in_progress_total', accent: 'border-t-amber-500' },
        { key: 'done', title: 'Done', totalKey: 'done_total', accent: 'border-t-emerald-500' },
    ];
</script>

<svelte:head>
    <title>{project ? `${project.name} · Roadmap` : 'Roadmap'} · Rungu</title>
</svelte:head>

<div class="mx-auto max-w-7xl px-4 py-8">
    <!-- Header -->
    <div class="mb-6 flex items-center justify-between gap-4">
        <div>
            <Button variant="link" size="sm" href="/board/{slug}" class="mb-1 px-0 text-muted-foreground">
                ← Back to board
            </Button>
            <h1 class="text-2xl font-bold tracking-tight">
                {project ? project.name : 'Roadmap'}
            </h1>
            {#if project?.description}
                <p class="mt-1 text-sm text-muted-foreground">{project.description}</p>
            {/if}
        </div>
    </div>

    {#if loading}
        <!-- Skeleton: three columns of placeholder cards -->
        <div class="grid gap-4 md:grid-cols-3">
            {#each Array(3) as _, i (i)}
                <div class="space-y-3">
                    <Skeleton class="h-6 w-32" />
                    {#each Array(3) as _, j (j)}
                        <Skeleton class="h-24 w-full" />
                    {/each}
                </div>
            {/each}
        </div>
    {:else if error}
        <Card.Root class="py-12 text-center">
            <Card.Content class="pt-6">
                <p class="text-muted-foreground">{error}</p>
                <Button variant="link" href="/">Go home</Button>
            </Card.Content>
        </Card.Root>
    {:else if roadmap}
        <!-- Three-column board. Stacks to a single column on mobile (md:grid-cols-3). -->
        <div class="grid gap-4 md:grid-cols-3">
            {#each columns as col (col.key)}
                <div class="flex flex-col gap-3 rounded-lg border-t-4 {col.accent} bg-muted/30 p-3">
                    <div class="flex items-center justify-between">
                        <h2 class="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
                            {col.title}
                        </h2>
                        <span class="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                            {roadmap[col.totalKey]}
                        </span>
                    </div>

                    {#if roadmap[col.key].length === 0}
                        <!-- Per-column empty state -->
                        <p class="py-8 text-center text-sm text-muted-foreground">
                            Nothing here yet.
                        </p>
                    {:else}
                        {#each roadmap[col.key] as post (post.id)}
                            <PostCard {post} {slug} />
                        {/each}
                    {/if}
                </div>
            {/each}
        </div>

        <!-- Show more: reveal the next 10 posts per bucket (server caps at 50). -->
        {#if [roadmap.planned_total, roadmap.in_progress_total, roadmap.done_total].some((t) => t > bucketLimit) && bucketLimit < 50}
            <div class="mt-6 text-center">
                <Button variant="outline" onclick={expandBuckets}>
                    Show more
                </Button>
            </div>
        {/if}
    {/if}
</div>
