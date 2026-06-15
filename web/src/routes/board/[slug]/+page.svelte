<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, PostDetail, PostStatus, PostCategory } from '$lib/api/types';
    import PostCard from '$lib/components/PostCard.svelte';
    import PostForm from '$lib/components/PostForm.svelte';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';
    import { cn } from '$lib/utils';

    let { params } = $props();
    let slug = $derived(params.slug);

    let project = $state<Project | null>(null);
    let posts = $state<PostDetail[]>([]);
    let total = $state(0);
    let loading = $state(true);
    let error = $state('');

    let sort = $state('newest');
    let statusFilter = $state<PostStatus | ''>('');
    let categoryFilter = $state<PostCategory | ''>('');
    let searchQuery = $state('');
    let showForm = $state(false);
    let authed = $state(false);

    const sortOptions = [
        { value: 'newest', label: 'Newest' },
        { value: 'most_votes', label: 'Most Voted' },
        { value: 'recently_updated', label: 'Recently Updated' },
    ];

    const statusOptions: { value: PostStatus; label: string }[] = [
        { value: 'open', label: 'Open' },
        { value: 'planned', label: 'Planned' },
        { value: 'in_progress', label: 'In Progress' },
        { value: 'done', label: 'Done' },
        { value: 'declined', label: 'Declined' },
    ];

    const categoryOptions: { value: PostCategory; label: string }[] = [
        { value: 'feedback', label: '💬 Feedback' },
        { value: 'bug', label: '🐛 Bug' },
        { value: 'feature', label: '✨ Feature' },
        { value: 'question', label: '❓ Question' },
    ];

    async function loadBoard() {
        loading = true;
        error = '';
        try {
            project = await api.getProject(slug);
            const result = await api.listPosts(slug, {
                sort,
                status: statusFilter || undefined,
                category: categoryFilter || undefined,
                q: searchQuery || undefined,
                per_page: 50,
            });
            posts = result.data;
            total = result.pagination.total;
        } catch (e) {
            error = e instanceof ApiError && e.status === 404 ? 'Project not found' : 'Failed to load board';
        } finally {
            loading = false;
        }
    }

    onMount(async () => {
        try {
            await api.getCurrentUser();
            authed = true;
        } catch {
            authed = false;
        }
        await loadBoard();
    });

    async function handleCreatePost(data: { title: string; description: string; category: PostCategory }) {
        await api.createPost(slug, data);
        showForm = false;
        await loadBoard();
    }

    $effect(() => {
        if (project) loadBoard();
    });
</script>

<svelte:head>
    <title>{project?.name ?? 'Board'} — Rungu</title>
</svelte:head>

{#if loading && !project}
    <div class="space-y-4">
        {#each Array(3) as _}
            <Skeleton class="h-24 w-full rounded-xl" />
        {/each}
    </div>
{:else if error}
    <div class="py-16 text-center">
        <p class="text-lg text-muted-foreground">{error}</p>
        <Button variant="link" href="/">← Back to boards</Button>
    </div>
{:else if project}
    <div class="mb-6">
        <Button variant="link" size="sm" href="/" class="px-0 text-muted-foreground">← All boards</Button>
        <h1 class="mt-2 text-2xl font-bold">{project.name}</h1>
        {#if project.description}
            <p class="mt-1 text-sm text-muted-foreground">{project.description}</p>
        {/if}
    </div>

    <div class="grid gap-6 lg:grid-cols-[1fr_280px]">
        <!-- Main -->
        <div>
            <div class="mb-4 flex flex-wrap items-center gap-2">
                <Input
                    bind:value={searchQuery}
                    placeholder="Search..."
                    class="flex-1"
                />
                <select bind:value={sort} class="rounded-md border border-input bg-background px-3 py-2 text-sm">
                    {#each sortOptions as opt (opt.value)}
                        <option value={opt.value}>{opt.label}</option>
                    {/each}
                </select>
            </div>

            {#if statusFilter || categoryFilter}
                <div class="mb-4 flex flex-wrap gap-2">
                    {#if statusFilter}
                        <Button variant="secondary" size="xs" onclick={() => (statusFilter = '')}>
                            {statusOptions.find((s) => s.value === statusFilter)?.label} ✕
                        </Button>
                    {/if}
                    {#if categoryFilter}
                        <Button variant="secondary" size="xs" onclick={() => (categoryFilter = '')}>
                            {categoryOptions.find((c) => c.value === categoryFilter)?.label} ✕
                        </Button>
                    {/if}
                </div>
            {/if}

            <div class="space-y-3">
                {#each posts as post (post.id)}
                    <PostCard {post} {slug} />
                {/each}
            </div>

            {#if posts.length === 0}
                <div class="rounded-xl border border-dashed border-border py-12 text-center">
                    <p class="text-muted-foreground">{searchQuery ? 'No posts match your search.' : 'No posts yet.'}</p>
                </div>
            {/if}
        </div>

        <!-- Sidebar -->
        <div class="space-y-4">
            {#if authed}
                <Button class="w-full" onclick={() => (showForm = !showForm)}>
                    {showForm ? 'Cancel' : '+ New Post'}
                </Button>

                {#if showForm}
                    <PostForm {slug} onsubmit={handleCreatePost} />
                {/if}
            {:else}
                <Card.Root>
                    <Card.Content class="pt-6 text-center text-sm text-muted-foreground">
                        <a href="/login" class="font-medium text-primary hover:underline">Login</a> to post and vote
                    </Card.Content>
                </Card.Root>
            {/if}

            <Card.Root class="p-3">
                <h3 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Categories</h3>
                <div class="flex flex-col gap-1">
                    {#each categoryOptions as cat (cat.value)}
                        <button
                            onclick={() => (categoryFilter = categoryFilter === cat.value ? '' : cat.value)}
                            class={cn(
                                'rounded-md px-2 py-1 text-left text-sm transition-colors',
                                categoryFilter === cat.value
                                    ? 'bg-primary/10 font-medium text-primary'
                                    : 'text-muted-foreground hover:bg-muted',
                            )}
                        >
                            {cat.label}
                        </button>
                    {/each}
                </div>
            </Card.Root>

            <Card.Root class="p-3">
                <h3 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Status</h3>
                <div class="flex flex-col gap-1">
                    {#each statusOptions as st (st.value)}
                        <button
                            onclick={() => (statusFilter = statusFilter === st.value ? '' : st.value)}
                            class={cn(
                                'rounded-md px-2 py-1 text-left text-sm capitalize transition-colors',
                                statusFilter === st.value
                                    ? 'bg-primary/10 font-medium text-primary'
                                    : 'text-muted-foreground hover:bg-muted',
                            )}
                        >
                            {st.label}
                        </button>
                    {/each}
                </div>
            </Card.Root>
        </div>
    </div>
{/if}
