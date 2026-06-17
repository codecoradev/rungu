<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, PostDetail } from '$lib/api/types';
    import CategoryBadge from '$lib/components/CategoryBadge.svelte';
    import { Button } from '$lib/components/ui/button';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';

    let { params } = $props();
    let slug = $derived(params.slug);

    let project = $state<Project | null>(null);
    let posts = $state<PostDetail[]>([]);
    let total = $state(0);
    let page = $state(1);
    const perPage = 20;
    let totalPages = $state(1);
    let loading = $state(true);
    let error = $state('');

    onMount(async () => {
        await Promise.all([loadProject(), loadChangelog()]);
    });

    async function loadProject() {
        try {
            project = await api.getProject(slug);
        } catch {
            // Header is optional; the changelog fetch owns the 404 path.
        }
    }

    async function loadChangelog() {
        loading = true;
        error = '';
        try {
            const res = await api.getChangelog(slug, { page, per_page: perPage });
            posts = res.data;
            total = res.pagination.total;
            totalPages = res.pagination.total_pages;
        } catch (e) {
            if (e instanceof ApiError && e.status === 404) {
                error = 'Board not found.';
            } else {
                error = 'Failed to load changelog. Please try again.';
            }
        } finally {
            loading = false;
        }
    }

    async function nextPage() {
        if (page >= totalPages) return;
        page += 1;
        await loadChangelog();
        scrollTo({ top: 0, behavior: 'smooth' });
    }

    async function prevPage() {
        if (page <= 1) return;
        page -= 1;
        await loadChangelog();
        scrollTo({ top: 0, behavior: 'smooth' });
    }

    // Group posts by their `updated_at` date (the "shipped" date) for timeline
    // date headers. A post moves to `done` via update_post_status, which bumps
    // updated_at — so this is the closest proxy we have to a ship date.
    function dateKey(iso: string): string {
        return new Date(iso).toLocaleDateString(undefined, {
            year: 'numeric',
            month: 'long',
            day: 'numeric',
        });
    }

    // Partition posts into date buckets while preserving order.
    let buckets = $derived.by(() => {
        const out: { date: string; posts: PostDetail[] }[] = [];
        for (const p of posts) {
            const key = dateKey(p.updated_at);
            const last = out[out.length - 1];
            if (last && last.date === key) {
                last.posts.push(p);
            } else {
                out.push({ date: key, posts: [p] });
            }
        }
        return out;
    });
</script>

<svelte:head>
    <title>{project ? `${project.name} · Changelog` : 'Changelog'} · Rungu</title>
</svelte:head>

<div class="mx-auto max-w-3xl px-4 py-8">
    <!-- Header -->
    <div class="mb-6">
        <Button variant="link" size="sm" href="/board/{slug}" class="mb-1 px-0 text-muted-foreground">
            ← Back to board
        </Button>
        <h1 class="text-2xl font-bold tracking-tight">
            {project ? project.name : 'Changelog'}
        </h1>
        <p class="mt-1 text-sm text-muted-foreground">
            What's shipped — updated as feedback moves to <span class="font-medium">done</span>.
        </p>
    </div>

    {#if loading}
        <div class="space-y-4">
            {#each Array(4) as _, i (i)}
                <Skeleton class="h-20 w-full" />
            {/each}
        </div>
    {:else if error}
        <Card.Root class="py-12 text-center">
            <Card.Content class="pt-6">
                <p class="text-muted-foreground">{error}</p>
                <Button variant="link" href="/">Go home</Button>
            </Card.Content>
        </Card.Root>
    {:else if posts.length === 0}
        <!-- Empty state -->
        <Card.Root class="py-16 text-center">
            <Card.Content class="pt-6">
                <p class="text-lg font-medium">No shipped items yet</p>
                <p class="mt-1 text-sm text-muted-foreground">
                    Posts appear here once their status moves to <span class="font-medium">done</span>.
                </p>
                <Button variant="link" href="/board/{slug}">Browse the board</Button>
            </Card.Content>
        </Card.Root>
    {:else}
        <!-- Timeline -->
        <div class="space-y-8">
            {#each buckets as bucket (bucket.date)}
                <section>
                    <div class="mb-3 flex items-center gap-3">
                        <h2 class="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
                            {bucket.date}
                        </h2>
                        <div class="h-px flex-1 bg-border"></div>
                    </div>
                    <div class="space-y-3">
                        {#each bucket.posts as post (post.id)}
                            <a href="/board/{slug}/post/{post.id}" class="block">
                                <Card.Root class="transition-shadow hover:shadow-md">
                                    <Card.Content class="flex items-start gap-3 p-4">
                                        <div class="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/10 text-emerald-600">
                                            <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
                                                <path fill-rule="evenodd" d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z" clip-rule="evenodd" />
                                            </svg>
                                        </div>
                                        <div class="min-w-0 flex-1">
                                            <div class="flex items-center gap-2">
                                                <CategoryBadge category={post.category} />
                                            </div>
                                            <h3 class="mt-1 font-semibold leading-tight">{post.title}</h3>
                                            {#if post.description}
                                                <p class="mt-1 line-clamp-2 text-sm text-muted-foreground">{post.description}</p>
                                            {/if}
                                            <div class="mt-2 flex items-center gap-3 text-xs text-muted-foreground">
                                                <span>{post.vote_count} votes</span>
                                                {#if post.comment_count > 0}
                                                    <span>·</span>
                                                    <span>{post.comment_count} comments</span>
                                                {/if}
                                            </div>
                                        </div>
                                    </Card.Content>
                                </Card.Root>
                            </a>
                        {/each}
                    </div>
                </section>
            {/each}
        </div>

        <!-- Pagination -->
        {#if totalPages > 1}
            <div class="mt-8 flex items-center justify-center gap-3">
                <Button variant="outline" size="sm" disabled={page <= 1} onclick={prevPage}>
                    ← Previous
                </Button>
                <span class="text-sm text-muted-foreground">Page {page} of {totalPages}</span>
                <Button variant="outline" size="sm" disabled={page >= totalPages} onclick={nextPage}>
                    Next →
                </Button>
            </div>
        {/if}
    {/if}
</div>
