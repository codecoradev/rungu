<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, CurrentUser } from '$lib/api/types';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';
    import { timeAgo } from '$lib/utils';

    let projects = $state<Project[]>([]);
    let user = $state<CurrentUser | null>(null);
    let loading = $state(true);
    let error = $state('');

    // Create form
    let newName = $state('');
    let newSlug = $state('');
    let newDesc = $state('');
    let creating = $state(false);
    let createError = $state('');

    // Edit state
    let editing = $state<string | null>(null);
    let editName = $state('');
    let editDesc = $state('');

    const isAdmin = $derived(user?.role === 'admin');

    async function loadData() {
        loading = true;
        try {
            user = await api.getCurrentUser();
            if (!isAdmin) {
                error = 'Admin access required';
                return;
            }
            projects = await api.listProjects();
        } catch {
            error = 'Failed to load';
        } finally {
            loading = false;
        }
    }

    onMount(loadData);

    function slugify(name: string): string {
        return name
            .toLowerCase()
            .replace(/[^a-z0-9\s-]/g, '')
            .replace(/\s+/g, '-')
            .replace(/-+/g, '-')
            .replace(/^-|-$/g, '');
    }

    async function handleCreate(e: Event) {
        e.preventDefault();
        if (!newName.trim()) return;
        creating = true;
        createError = '';
        try {
            const project = await api.createProject({
                name: newName.trim(),
                slug: newSlug.trim() || slugify(newName),
                description: newDesc.trim() || undefined,
            });
            projects = [project, ...projects];
            newName = '';
            newSlug = '';
            newDesc = '';
        } catch (e) {
            createError = e instanceof ApiError ? e.message : 'Failed to create project';
        } finally {
            creating = false;
        }
    }

    function startEdit(project: Project) {
        editing = project.slug;
        editName = project.name;
        editDesc = project.description;
    }

    async function saveEdit(slug: string) {
        try {
            const updated = await api.updateProject(slug, {
                name: editName.trim() || undefined,
                description: editDesc || undefined,
            });
            projects = projects.map((p) => (p.slug === slug ? updated : p));
            editing = null;
        } catch (e) {
            error = 'Failed to update project';
        }
    }

    async function handleDelete(slug: string) {
        if (!confirm(`Delete project "${slug}"? This will cascade-delete all posts, votes, and comments.`)) return;
        try {
            await api.deleteProject(slug);
            projects = projects.filter((p) => p.slug !== slug);
        } catch {
            error = 'Failed to delete project';
        }
    }
</script>

<svelte:head>
    <title>Admin — Rungu</title>
</svelte:head>

{#if loading}
    <div class="space-y-4">
        <Skeleton class="h-32 w-full" />
        <Skeleton class="h-20 w-full" />
    </div>
{:else if !isAdmin}
    <div class="py-16 text-center">
        <p class="text-lg text-muted-foreground">{error || 'Access denied'}</p>
        <Button variant="link" href="/">← Home</Button>
    </div>
{:else}
    <div class="mb-6">
        <h1 class="text-2xl font-bold">Admin</h1>
        <p class="mt-1 text-sm text-muted-foreground">Manage projects and their settings</p>
    </div>

    {#if error}
        <p class="mb-4 text-sm text-destructive">{error}</p>
    {/if}

    <!-- Create project -->
    <Card.Root class="mb-6">
        <Card.Header>
            <Card.Title class="text-base">New Project</Card.Title>
        </Card.Header>
        <Card.Content>
            <form onsubmit={handleCreate} class="space-y-3">
                {#if createError}
                    <p class="text-sm text-destructive">{createError}</p>
                {/if}
                <div class="grid gap-3 sm:grid-cols-2">
                    <Input bind:value={newName} placeholder="Project name" required oninput={() => { if (!newSlug) {} }} />
                    <Input
                        bind:value={newSlug}
                        placeholder="slug (auto from name)"
                        oninput={() => {}}
                    />
                </div>
                <Textarea bind:value={newDesc} placeholder="Description (optional)" rows="2" />
                <Button type="submit" disabled={creating || !newName.trim()}>
                    {creating ? 'Creating...' : 'Create Project'}
                </Button>
            </form>
        </Card.Content>
    </Card.Root>

    <!-- Project list -->
    <div class="space-y-3">
        <h2 class="text-sm font-semibold text-muted-foreground">Projects ({projects.length})</h2>

        {#each projects as project (project.id)}
            <Card.Root>
                {#if editing === project.slug}
                    <Card.Content class="space-y-3 pt-6">
                        <Input bind:value={editName} placeholder="Name" />
                        <Textarea bind:value={editDesc} placeholder="Description" rows="2" />
                        <div class="flex gap-2">
                            <Button size="sm" onclick={() => saveEdit(project.slug)}>Save</Button>
                            <Button variant="outline" size="sm" onclick={() => (editing = null)}>Cancel</Button>
                        </div>
                    </Card.Content>
                {:else}
                    <Card.Header class="flex-row items-start justify-between">
                        <div class="min-w-0 flex-1">
                            <Card.Title class="text-base">
                                {project.name}
                                <code class="ml-2 rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">{project.slug}</code>
                            </Card.Title>
                            {#if project.description}
                                <Card.Description class="mt-1">{project.description}</Card.Description>
                            {/if}
                            <p class="mt-1 text-xs text-muted-foreground">Created {timeAgo(project.created_at)}</p>
                        </div>
                        <div class="flex shrink-0 gap-1">
                            <Button variant="ghost" size="xs" href={`/board/${project.slug}`}>View</Button>
                            <Button variant="ghost" size="xs" onclick={() => startEdit(project)}>Edit</Button>
                            <Button variant="ghost" size="xs" class="text-destructive" onclick={() => handleDelete(project.slug)}>Delete</Button>
                        </div>
                    </Card.Header>
                {/if}
            </Card.Root>
        {/each}

        {#if projects.length === 0}
            <p class="py-8 text-center text-sm text-muted-foreground">No projects yet. Create one above.</p>
        {/if}
    </div>
{/if}
