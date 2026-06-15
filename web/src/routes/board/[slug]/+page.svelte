<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, PostDetail, PostStatus, PostCategory } from '$lib/api/types';
    import PostCard from '$lib/components/PostCard.svelte';
    import PostForm from '$lib/components/PostForm.svelte';
    import { cn } from '$lib/utils';

    let { params } = $props();
    let slug = $derived(params.slug);

    let project = $state<Project | null>(null);
    let posts = $state<PostDetail[]>([]);
    let total = $state(0);
    let loading = $state(true);
    let error = $state('');

    // Filters
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

    // Reload when filters change
    $effect(() => {
        if (project) loadBoard();
    });
</script>

<svelte:head>
    <title>{project?.name ?? 'Board'} — Rungu</title>
</svelte:head>

{#if loading && !project}
    <div class="py-16 text-center">
        <div class="inline-block size-8 animate-spin rounded-full border-2 border-gray-200 border-t-brand-600"></div>
    </div>
{:else if error}
    <div class="py-16 text-center">
        <p class="text-lg text-gray-500">{error}</p>
        <a href="/" class="mt-3 inline-block text-sm text-brand-600 hover:underline">← Back to boards</a>
    </div>
{:else if project}
    <!-- Header -->
    <div class="mb-6">
        <a href="/" class="text-sm text-gray-400 hover:text-gray-600">← All boards</a>
        <h1 class="mt-2 text-2xl font-bold text-gray-900">{project.name}</h1>
        {#if project.description}
            <p class="mt-1 text-sm text-gray-500">{project.description}</p>
        {/if}
    </div>

    <div class="grid gap-6 lg:grid-cols-[1fr_280px]">
        <!-- Main: posts list -->
        <div>
            <!-- Toolbar -->
            <div class="mb-4 flex flex-wrap items-center gap-2">
                <input
                    bind:value={searchQuery}
                    oninput={() => {}}
                    type="text"
                    placeholder="Search..."
                    class="flex-1 rounded-lg border border-gray-200 px-3 py-1.5 text-sm outline-none focus:border-brand-400"
                />
                <select bind:value={sort} class="rounded-lg border border-gray-200 px-3 py-1.5 text-sm">
                    {#each sortOptions as opt (opt.value)}
                        <option value={opt.value}>{opt.label}</option>
                    {/each}
                </select>
            </div>

            <!-- Filter chips -->
            <div class="mb-4 flex flex-wrap gap-2">
                {#if statusFilter}
                    {@const opt = statusOptions.find((s) => s.value === statusFilter)}
                    <button
                        onclick={() => (statusFilter = '')}
                        class="inline-flex items-center gap-1 rounded-full bg-brand-50 px-2.5 py-0.5 text-xs font-medium text-brand-700"
                    >
                        {opt?.label} ✕
                    </button>
                {/if}
                {#if categoryFilter}
                    {@const opt = categoryOptions.find((c) => c.value === categoryFilter)}
                    <button
                        onclick={() => (categoryFilter = '')}
                        class="inline-flex items-center gap-1 rounded-full bg-brand-50 px-2.5 py-0.5 text-xs font-medium text-brand-700"
                    >
                        {opt?.label} ✕
                    </button>
                {/if}
            </div>

            <!-- Posts -->
            <div class="space-y-3">
                {#each posts as post (post.id)}
                    <PostCard {post} {slug} />
                {/each}
            </div>

            {#if posts.length === 0}
                <div class="rounded-xl border border-dashed border-gray-200 py-12 text-center">
                    <p class="text-gray-400">{searchQuery ? 'No posts match your search.' : 'No posts yet.'}</p>
                </div>
            {/if}
        </div>

        <!-- Sidebar -->
        <div class="space-y-4">
            {#if authed}
                <button
                    onclick={() => (showForm = !showForm)}
                    class="w-full rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700"
                >
                    {showForm ? 'Cancel' : '+ New Post'}
                </button>

                {#if showForm}
                    <PostForm {slug} onsubmit={handleCreatePost} />
                {/if}
            {:else}
                <div class="rounded-lg border border-gray-200 bg-gray-50 p-3 text-center text-sm text-gray-500">
                    <a href="/login" class="font-medium text-brand-600 hover:underline">Login</a> to post and vote
                </div>
            {/if}

            <!-- Category filters -->
            <div class="rounded-xl border border-gray-200 bg-white p-3">
                <h3 class="mb-2 text-xs font-semibold uppercase text-gray-400">Categories</h3>
                <div class="flex flex-col gap-1">
                    {#each categoryOptions as cat (cat.value)}
                        <button
                            onclick={() => (categoryFilter = categoryFilter === cat.value ? '' : cat.value)}
                            class={cn(
                                'rounded-md px-2 py-1 text-left text-sm transition-colors',
                                categoryFilter === cat.value
                                    ? 'bg-brand-50 font-medium text-brand-700'
                                    : 'text-gray-600 hover:bg-gray-50',
                            )}
                        >
                            {cat.label}
                        </button>
                    {/each}
                </div>
            </div>

            <!-- Status filters -->
            <div class="rounded-xl border border-gray-200 bg-white p-3">
                <h3 class="mb-2 text-xs font-semibold uppercase text-gray-400">Status</h3>
                <div class="flex flex-col gap-1">
                    {#each statusOptions as st (st.value)}
                        <button
                            onclick={() => (statusFilter = statusFilter === st.value ? '' : st.value)}
                            class={cn(
                                'rounded-md px-2 py-1 text-left text-sm capitalize transition-colors',
                                statusFilter === st.value
                                    ? 'bg-brand-50 font-medium text-brand-700'
                                    : 'text-gray-600 hover:bg-gray-50',
                            )}
                        >
                            {st.label}
                        </button>
                    {/each}
                </div>
            </div>
        </div>
    </div>
{/if}
