<script lang="ts">
    import { onMount } from 'svelte';
    import { api } from '$lib/api/client';
    import type { Project } from '$lib/api/types';
    import * as Card from '$lib/components/ui/card';

    let projects = $state<Project[]>([]);

    onMount(async () => {
        try {
            projects = await api.listProjects();
        } catch {
            projects = [];
        }
    });
</script>

<svelte:head>
    <title>Rungu — Lightweight Feedback Board</title>
    <meta name="description" content="Collect feature requests, bug reports, and suggestions." />
</svelte:head>

<div class="py-12 text-center">
    <h1 class="text-4xl font-bold">Dengar. Prioritaskan. Bangun.</h1>
    <p class="mx-auto mt-3 max-w-lg text-muted-foreground">
        A lightweight, self-hosted feedback board. Submit, vote, and discuss — all in one place.
    </p>
</div>

<div class="py-6">
    <h2 class="mb-4 text-lg font-semibold">Boards</h2>

    {#if projects.length === 0}
        <div class="rounded-xl border border-dashed border-border py-12 text-center">
            <p class="text-muted-foreground">No projects yet.</p>
        </div>
    {:else}
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {#each projects as project (project.slug)}
                <a href={`/board/${project.slug}`}>
                    <Card.Root class="transition-shadow hover:shadow-md">
                        <Card.Header>
                            <Card.Title>{project.name}</Card.Title>
                            {#if project.description}
                                <Card.Description class="line-clamp-2">{project.description}</Card.Description>
                            {/if}
                        </Card.Header>
                        <Card.Footer>
                            <span class="text-xs font-medium text-primary">View board →</span>
                        </Card.Footer>
                    </Card.Root>
                </a>
            {/each}
        </div>
    {/if}
</div>
