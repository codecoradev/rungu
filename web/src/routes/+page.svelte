<script lang="ts">
    import { onMount } from 'svelte';
    import { api } from '$lib/api/api';
    import type { Project } from '$lib/api/types';

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

<!-- Hero -->
<div class="py-12 text-center">
    <h1 class="text-4xl font-bold text-gray-900">Dengar. Prioritaskan. Bangun.</h1>
    <p class="mx-auto mt-3 max-w-lg text-gray-500">
        A lightweight, self-hosted feedback board. Submit, vote, and discuss — all in one place.
    </p>
</div>

<!-- Projects -->
<div class="py-6">
    <h2 class="mb-4 text-lg font-semibold text-gray-700">Boards</h2>

    {#if projects.length === 0}
        <div class="rounded-xl border border-dashed border-gray-200 py-12 text-center">
            <p class="text-gray-400">No projects yet.</p>
        </div>
    {:else}
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {#each projects as project (project.slug)}
                <a
                    href={`/board/${project.slug}`}
                    class="rounded-xl border border-gray-200 bg-white p-5 transition-shadow hover:shadow-md"
                >
                    <h3 class="font-semibold text-gray-900">{project.name}</h3>
                    {#if project.description}
                        <p class="mt-1 text-sm text-gray-500 line-clamp-2">{project.description}</p>
                    {/if}
                    <div class="mt-3 text-xs font-medium text-brand-600">View board →</div>
                </a>
            {/each}
        </div>
    {/if}
</div>
