<script lang="ts">
    import { branding } from '$lib/branding.svelte';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, PostDetail, PostStatus, PostCategory } from '$lib/api/types';
    import PostCard from '$lib/components/PostCard.svelte';
    import PostForm from '$lib/components/PostForm.svelte';
    import SortTabs from '$lib/components/SortTabs.svelte';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';
    import { cn } from '$lib/utils';
    import { toastError } from '$lib/toast.svelte';
    import { LoaderCircle, Plus, X } from '@lucide/svelte';
    import CircleAlert from '@lucide/svelte/icons/circle-alert';

    let { params } = $props();
    let slug = $derived(params.slug);

    let project = $state<Project | null>(null);
    let posts = $state<PostDetail[]>([]);
    let total = $state(0);
    let loading = $state(true);
    // True while re-fetching with posts already on screen (filter/search refill).
    // Gates the dimming overlay so the first load still shows skeletons. #148
    let refetching = $state(false);
    // Monotonic guard against stale loads (#173): every loadBoard() bumps the
    // generation; only the latest generation may commit its response.
    let loadGeneration = $state(0);
    let error = $state('');

    let sort = $state('newest');
    // URL is the source of truth for sort (#203): hydrate from ?sort= on boot
    // and push changes back so reload/share preserves the state. Falls back to
    // the default for unknown values (BE also clamps unknown → newest).
    // (Keep this list in sync with `sortOptions` below; inlined here because
    // this initializer runs before that const exists.)
    {
        const fromUrl = new URLSearchParams(window.location.search).get('sort');
        if (fromUrl && ['newest', 'trending', 'most_votes', 'recently_updated'].includes(fromUrl)) sort = fromUrl;
    }
    $effect(() => {
        if (!initialized) return;
        const url = new URL(window.location.href);
        if (sort === 'newest') url.searchParams.delete('sort');
        else url.searchParams.set('sort', sort);
        if (url.href !== window.location.href) history.replaceState(history.state, '', url.href);
    });
    let statusFilter = $state<PostStatus | ''>('');
    let categoryFilter = $state<PostCategory | ''>('');
    // Sidebar / filter counts (#202): refreshed with every board load.
    let counts = $state<{ by_status: Record<string, number>; by_category: Record<string, number> } | null>(null);
    let searchQuery = $state('');
    let showForm = $state(false);
    let dialogEl: HTMLDivElement | null = $state(null);
    let authed = $state(false);
    // Mobile toolbar: filter panel visibility (see mobile toolbar markup).
    let showFilters = $state(false);
    // Set true once the initial onMount load finishes; gates the filter $effect
    // so it doesn't fire before the first board fetch completes. Declared up
    // here (before onMount) to avoid a forward reference / TDZ smell.
    let initialized = $state(false);

    // Keyboard-shortcut focus state. Tracks the currently-focused post card
    // so j/k navigation and Enter/v shortcuts have a target. -1 = none focused.
    let focusedPostIndex = $state(-1);

    const sortOptions = [
        { value: 'newest', label: 'Newest' },
        { value: 'trending', label: 'Trending' },
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
        { value: 'feedback', label: 'Feedback' },
        { value: 'bug', label: 'Bug' },
        { value: 'feature', label: 'Feature' },
        { value: 'question', label: 'Question' },
    ];

    async function loadBoard() {
        // Filter/search refill: posts are already rendered — dim them instead
        // of flashing skeletons, and reset pagination. #148
        refetching = posts.length > 0;
        loading = posts.length === 0;
        error = '';

        // Stale-response guard (#173): mutations and filter changes can fire
        // loadBoard() concurrently. Capture the generation; a newer load
        // invalidates this one, so an older response never overwrites it.
        const generation = ++loadGeneration;

        try {
            const projectData = await api.getProject(slug);
            const result = await api.listPosts(slug, {
                sort,
                status: statusFilter || undefined,
                category: categoryFilter || undefined,
                q: searchQuery || undefined,
                per_page: 50,
            });
            if (generation !== loadGeneration) return; // a newer load superseded this one
            project = projectData;
            posts = result.data;
            total = result.pagination.total;
            focusedPostIndex = -1;
            // Counts load best-effort; their absence must never break the board.
            api.getProjectCounts(slug).then((c) => (counts = c)).catch(() => {});
        } catch (e) {
            if (generation !== loadGeneration) return;
            error = e instanceof ApiError && e.status === 404 ? 'Project not found' : 'Failed to load board';
        } finally {
            if (generation === loadGeneration) {
                loading = false;
                refetching = false;
            }
        }
    }

    async function loadMore() {
        const before = posts.length;
        if (before >= total) return;
        try {
            const result = await api.listPosts(slug, {
                sort,
                status: statusFilter || undefined,
                category: categoryFilter || undefined,
                q: searchQuery || undefined,
                per_page: 50,
                page: Math.floor(before / 50) + 1,
            });
            // Merge, dedup by id (defensive against shifting sort orders)
            const seen = new Set(posts.map((p) => p.id));
            posts = [...posts, ...result.data.filter((p) => !seen.has(p.id))];
            total = result.pagination.total;
        } catch {
            toastError('Failed to load more posts');
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
        initialized = true;
    });

    /**
     * Board-scope keyboard-shortcut handler. Receives `rungu:shortcut`
     * events dispatched by +layout.svelte. Implements the board subset of
     * the registry (see $lib/shortcuts.ts).
     */
    function onShortcut(e: Event) {
        if (posts.length === 0 && (focusedPostIndex === -1)) {
            // Still allow `/` to focus search even when the list is empty.
        }
        const { key, authRequired } = (e as CustomEvent).detail as {
            key: string;
            scope: string;
            authRequired?: boolean;
        };

        switch (key) {
            case '/': {
                const el = document.getElementById('board-search');
                if (el instanceof HTMLElement) el.focus();
                break;
            }
            case 'c': {
                if (!authed) return;
                showForm = true;
                break;
            }
            case 'j': {
                if (posts.length === 0) return;
                focusedPostIndex = Math.min(focusedPostIndex + 1, posts.length - 1);
                scrollFocusedIntoView();
                break;
            }
            case 'k': {
                if (posts.length === 0) return;
                focusedPostIndex = Math.max(focusedPostIndex - 1, 0);
                scrollFocusedIntoView();
                break;
            }
            case 'Enter': {
                const post = posts[focusedPostIndex];
                if (post) goto(`/board/${slug}/post/${post.id}`);
                break;
            }
            case 'v': {
                // Voting always requires auth. `authRequired` is irrelevant
                // here (there's no anonymous vote path), so just guard on auth.
                if (!authed) return;
                const post = posts[focusedPostIndex];
                if (post) toggleVote(post);
                break;
            }
        }
    }

    function scrollFocusedIntoView() {
        if (focusedPostIndex < 0) return;
        // Defer to after DOM updates.
        queueMicrotask(() => {
            const el = document.querySelector(`[data-post-index="${focusedPostIndex}"]`);
            if (el instanceof HTMLElement) {
                el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }
        });
    }

    async function toggleVote(post: PostDetail) {
        try {
            const result = await api.toggleVote(post.id);
            post.user_voted = result.voted;
            post.vote_count = result.vote_count;
        } catch {
            // Silently ignore — vote failure isn't worth a banner here.
        }
    }

    onMount(() => {
        window.addEventListener('rungu:shortcut', onShortcut as EventListener);
        return () => window.removeEventListener('rungu:shortcut', onShortcut as EventListener);
    });

    async function handleCreatePost(data: { title: string; description: string; category: PostCategory }) {
        await api.createPost(slug, data);
        showForm = false;
        await loadBoard();
    }

    // Re-fetch when the slug changes (route param swap). We intentionally
    // read only `slug` inside this effect — reading `project`/`posts`/etc
    // would re-trigger the effect on our own state writes and cause loops.
    // `loadBoard()` is safe to call before `project` is set; it sets `project`
    // on first resolve. We also guard against running before mount completes
    // via the `initialized` flag set at the end of onMount.
    // Reload board when filters or slug change.
    // Search is debounced to avoid excessive API calls while typing.
    let searchTimer: ReturnType<typeof setTimeout> | null = null;
    $effect(() => {
        if (!initialized) return;
        // Touch all reactive deps so $effect re-runs when any changes
        void slug;
        void statusFilter;
        void categoryFilter;
        void sort;

        // Debounce search input
        const sq = searchQuery;
        if (searchTimer) clearTimeout(searchTimer);
        searchTimer = setTimeout(() => {
            loadBoard();
        }, sq ? 300 : 0);

        // Cleanup on re-run or unmount
        return () => {
            if (searchTimer) clearTimeout(searchTimer);
        };
    });
</script>

<svelte:head>
    <title>{(project?.name ?? 'Board') + ' — ' + branding.value.brandName}</title>
</svelte:head>

<!-- Dialog Esc handling must be window-level: focus may sit inside inputs. -->
<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && showForm) showForm = false; }} />

{#if loading && !project}
    <div class="space-y-4">
        {#each Array(3) as _}
            <Skeleton class="h-24 w-full rounded-xl" />
        {/each}
    </div>
{:else if error}
    <Card.Root class="py-12 text-center">
        <Card.Content class="flex flex-col items-center gap-3 pt-6">
            <div class="flex size-10 items-center justify-center rounded-full bg-destructive/10">
                <CircleAlert class="size-5 text-destructive" aria-hidden="true" />
            </div>
            <h1 class="text-lg font-semibold">{error}</h1>
            <Button variant="outline" size="sm" href="/" class="mt-2">← Back to boards</Button>
        </Card.Content>
    </Card.Root>
{:else if project}
    <div class="mb-4">
        <Button variant="link" size="sm" href="/" class="max-sm:h-11 px-0 text-muted-foreground">← All boards</Button>
        <div class="mt-2 flex flex-wrap items-end justify-between gap-3">
            <div>
                <h1 class="text-2xl font-bold">{project.name}</h1>
                {#if project.description}
                    <p class="mt-1 text-sm text-muted-foreground">{project.description}</p>
                {/if}
            </div>
            {#if authed}
                <Button class="max-sm:h-11" onclick={() => (showForm = true)}>
                    <Plus class="size-4" aria-hidden="true" /> New Post
                </Button>
            {/if}
        </div>
    </div>

    <!-- View tabs (Featurebase pattern): the board's three surfaces share one
         nav row so the list context is explicit and the sidebar stays content-only. -->
    <nav aria-label="Board views" class="mb-4 border-b">
        <div class="flex gap-1 overflow-x-auto" role="tablist">
            <a
                href="/board/{slug}"
                role="tab"
                aria-selected="true"
                class="whitespace-nowrap border-b-2 border-primary px-3 py-2 text-sm font-medium text-foreground max-sm:h-11 max-sm:inline-flex max-sm:items-center"
            >
                Feedback
            </a>
            <a
                href="/board/{slug}/roadmap"
                role="tab"
                aria-selected="false"
                class="whitespace-nowrap border-b-2 border-transparent px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground max-sm:h-11 max-sm:inline-flex max-sm:items-center"
            >
                Roadmap
            </a>
            <a
                href="/board/{slug}/changelog"
                role="tab"
                aria-selected="false"
                class="whitespace-nowrap border-b-2 border-transparent px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground max-sm:h-11 max-sm:inline-flex max-sm:items-center"
            >
                Changelog
            </a>
        </div>
    </nav>

    <!-- Mobile action toolbar: sticky, replaces the desktop sidebar below lg.
         Navigation lives in the view tabs; this row carries New Post + Filters
         only. Tap targets >= 44px (mobile-ux HIG minimum). -->
    <div class="sticky top-14 z-20 -mx-4 mb-4 border-b bg-background/95 px-4 py-2 backdrop-blur supports-[backdrop-filter]:bg-background/80 lg:hidden">
        <div class="flex items-center gap-2 overflow-x-auto">
            {#if authed}
                <Button
                    size="sm"
                    class="h-11 shrink-0"
                    onclick={() => (showForm = true)}
                >
                    <Plus class="size-4" aria-hidden="true" /> New Post
                </Button>
            {:else}
                <Button size="sm" class="h-11 shrink-0" href="/login">Login to post</Button>
            {/if}
            <Button
                variant="outline"
                size="sm"
                class="h-11 shrink-0"
                aria-expanded={showFilters}
                onclick={() => (showFilters = !showFilters)}
            >
                Filters{statusFilter || categoryFilter
                    ? ` (${(statusFilter ? 1 : 0) + (categoryFilter ? 1 : 0)})`
                    : ''} {showFilters ? '▴' : '▾'}
            </Button>
        </div>

        {#if showFilters}
            <!-- Collapsible filter panel: category + status as horizontal chip
                 sliders (one row each, swipeable), same state as the desktop
                 sidebar so both stay in sync. -->
            <div class="mt-2 space-y-2 border-t pt-2">
                <div>
                    <h3 class="mb-1 text-xs font-semibold uppercase text-muted-foreground">Category</h3>
                    <div class="flex gap-1 overflow-x-auto pb-1" role="group" aria-label="Filter by category">
                        {#each categoryOptions as cat (cat.value)}
                            <button
                                onclick={() => (categoryFilter = categoryFilter === cat.value ? '' : cat.value)}
                                aria-pressed={categoryFilter === cat.value}
                                class={cn(
                                    'min-h-11 shrink-0 whitespace-nowrap rounded-full border px-3 py-1 text-sm transition-colors',
                                    categoryFilter === cat.value
                                        ? 'border-primary bg-primary/10 font-medium text-primary'
                                        : 'border-input text-muted-foreground hover:bg-muted',
                                )}
                            >
                                {cat.label}{counts ? ` (${counts.by_category[cat.value] ?? 0})` : ''}
                            </button>
                        {/each}
                    </div>
                </div>
                <div>
                    <h3 class="mb-1 text-xs font-semibold uppercase text-muted-foreground">Status</h3>
                    <div class="flex gap-1 overflow-x-auto pb-1" role="group" aria-label="Filter by status">
                        {#each statusOptions as st (st.value)}
                            <button
                                onclick={() => (statusFilter = statusFilter === st.value ? '' : st.value)}
                                aria-pressed={statusFilter === st.value}
                                class={cn(
                                    'min-h-11 shrink-0 whitespace-nowrap rounded-full border px-3 py-1 text-sm capitalize transition-colors',
                                    statusFilter === st.value
                                        ? 'border-primary bg-primary/10 font-medium text-primary'
                                        : 'border-input text-muted-foreground hover:bg-muted',
                                )}
                            >
                                {st.label}{counts ? ` (${counts.by_status[st.value] ?? 0})` : ''}
                            </button>
                        {/each}
                    </div>
                </div>
            </div>
        {/if}
    </div>

    {#if showForm}
        <!-- Post form as modal dialog (#feedback): overlay + centered card,
             Esc / backdrop click to dismiss. Mobile toolbar now just toggles it. -->
        <div
            class="fixed inset-0 z-50 flex items-end justify-center bg-background/80 p-0 backdrop-blur-sm sm:items-center sm:p-4"
            role="presentation"
            onclick={(e) => {
                if (e.target === e.currentTarget) showForm = false;
            }}
        >
            <div
                bind:this={dialogEl}
                role="dialog"
                aria-modal="true"
                aria-label="New post"
                class="max-h-[92vh] w-full max-w-lg overflow-y-auto rounded-t-xl border bg-background p-4 shadow-lg sm:rounded-xl"
            >
                <div class="mb-3 flex items-center justify-between">
                    <span class="text-base font-semibold">New Post</span>
                    <Button variant="ghost" size="icon-sm" aria-label="Close" onclick={() => (showForm = false)}>
                        <X class="size-4" aria-hidden="true" />
                    </Button>
                </div>
                <PostForm {slug} onsubmit={handleCreatePost} showTitle={false} />
            </div>
        </div>
    {/if}

    <div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
        <!-- Main -->
        <div class="min-w-0">
            <div class="mb-4 flex flex-wrap items-center gap-2">
                <div class="relative min-w-0 flex-1">
                    <Input
                        id="board-search"
                        bind:value={searchQuery}
                        type="search"
                        placeholder="Search..."
                        class="max-sm:h-11"
                    />
                    {#if refetching && searchQuery}
                        <span
                            class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"
                            role="status"
                            aria-label="Searching"
                        >
                            <LoaderCircle class="size-4 animate-spin" aria-hidden="true" />
                        </span>
                    {/if}
                </div>
                <SortTabs options={sortOptions} value={sort} onchange={(v) => (sort = v)} />
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

            <!-- Flat hairline list (#201): rows carry their own dividers, so
                 the container only resets the last divider per page batch. -->
            <div class="transition-opacity duration-200 {refetching ? 'pointer-events-none opacity-40' : ''}">
                {#each posts as post, i (post.id)}
                    <div
                        data-post-index={i}
                        class={cn(
                            'rounded-lg transition-all',
                            focusedPostIndex === i ? 'ring-2 ring-primary ring-inset' : '',
                        )}
                    >
                        <PostCard {post} {slug} />
                    </div>
                {/each}
            </div>

            {#if posts.length > 0 && total > posts.length}
                <div class="mt-4 text-center">
                    <p class="mb-2 text-xs text-muted-foreground" aria-live="polite">
                        Showing {posts.length} of {total} posts
                    </p>
                    <Button variant="outline" size="sm" onclick={loadMore}>Show more</Button>
                </div>
            {:else if posts.length > 0 && total === posts.length && total > 50}
                <p class="mt-4 text-center text-xs text-muted-foreground">Showing all {total} posts</p>
            {/if}

            {#if posts.length === 0}
                <div class="rounded-xl border border-dashed border-border py-12 text-center">
                    {#if searchQuery || statusFilter || categoryFilter}
                        <p class="text-lg font-medium">No results</p>
                        <p class="mt-1 text-sm text-muted-foreground">No posts match your current filters.</p>
                        <Button
                            variant="outline"
                            size="sm"
                            class="mt-4"
                            onclick={() => {
                                searchQuery = '';
                                statusFilter = '';
                                categoryFilter = '';
                            }}
                        >
                            Clear all filters
                        </Button>
                    {:else if authed}
                        <p class="text-lg font-medium">No feedback yet</p>
                        <p class="mt-1 text-sm text-muted-foreground">
                            Be the first to share an idea, report a bug, or ask a question.
                        </p>
                        <Button class="mt-4" onclick={() => (showForm = true)}>+ Post the first feedback</Button>
                    {:else}
                        <p class="text-lg font-medium">No feedback yet</p>
                        <p class="mt-1 text-sm text-muted-foreground">
                            This board is brand new. Sign in to be the first to post.
                        </p>
                        <Button class="mt-4" href="/login">Login to post</Button>
                    {/if}
                </div>
            {/if}
        </div>

        <!-- Sidebar -->
        <!-- Sidebar: desktop-only content rail (mobile filters live in the
             sticky toolbar's chip sliders). Navigation lives in the view tabs;
             posting opens the dialog. -->
        <div class="hidden min-w-0 space-y-4 lg:block">
            {#if !authed}
                <Card.Root>
                    <Card.Content class="pt-6 text-center text-sm text-muted-foreground">
                        <a href="/login" class="font-medium text-primary hover:underline">Login</a> to post and vote
                    </Card.Content>
                </Card.Root>
            {/if}

            <Card.Root class="p-3">
                <h2 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Categories</h2>
                <div class="flex flex-col gap-1">
                    {#each categoryOptions as cat (cat.value)}
                        <button
                            onclick={() => (categoryFilter = categoryFilter === cat.value ? '' : cat.value)}
                            class={cn(
                                'flex min-h-11 items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-sm transition-colors',
                                categoryFilter === cat.value
                                    ? 'bg-primary/10 font-medium text-primary'
                                    : 'text-muted-foreground hover:bg-muted',
                            )}
                        >
                            <span>{cat.label}</span>
                            <span class="text-xs tabular-nums text-muted-foreground">{counts?.by_category[cat.value] ?? 0}</span>
                        </button>
                    {/each}
                </div>
            </Card.Root>

            <Card.Root class="p-3">
                <h2 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Status</h2>
                <div class="flex flex-col gap-1">
                    {#each statusOptions as st (st.value)}
                        <button
                            onclick={() => (statusFilter = statusFilter === st.value ? '' : st.value)}
                            class={cn(
                                'flex min-h-11 items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-sm capitalize transition-colors',
                                statusFilter === st.value
                                    ? 'bg-primary/10 font-medium text-primary'
                                    : 'text-muted-foreground hover:bg-muted',
                            )}
                        >
                            <span>{st.label}</span>
                            <span class="text-xs tabular-nums text-muted-foreground">{counts?.by_status[st.value] ?? 0}</span>
                        </button>
                    {/each}
                </div>
            </Card.Root>
        </div>
    </div>
{/if}
