<script lang="ts">
    import { branding } from '$lib/branding.svelte';
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { api } from '$lib/api/client';
    import { session } from '$lib/session.svelte';
    import {
        SORT_OPTIONS,
        createBoardState,
        hrefWithSort,
        sortFromSearch,
        PAGE_SIZE,
        type Sort,
    } from '$lib/board/state.svelte';
    import type { PostStatus, PostCategory } from '$lib/api/types';
    import PostCard from '$lib/components/PostCard.svelte';
    import PostForm from '$lib/components/PostForm.svelte';
    import SortTabs from '$lib/components/SortTabs.svelte';
    import BoardTabs from '$lib/components/BoardTabs.svelte';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';
    import { page } from '$app/state';
    import { cn, loginHref } from '$lib/utils';
    import { toastError } from '$lib/toast.svelte';
    import { LoaderCircle, Plus, X } from '@lucide/svelte';
    import CircleAlert from '@lucide/svelte/icons/circle-alert';
    import { Dialog } from 'bits-ui';

    let { params } = $props();
    let slug = $derived(params.slug);
    const loginUrl = $derived(loginHref(page.url.pathname + page.url.search));

    // Everything the board does with data (loading, the stale-response guard,
    // paging, counts, filters, voting) lives in the state module; this page
    // renders it and forwards events. The URL is the source of truth for the
    // sort (#203): hydrate from ?sort= on boot, push changes back below.
    const board = createBoardState(api, {
        initialSort: sortFromSearch(window.location.search),
        onLoadMoreError: () => toastError('Failed to load more posts'),
    });

    const project = $derived(board.project);
    const posts = $derived(board.posts);
    const total = $derived(board.total);
    const loading = $derived(board.loading);
    const refetching = $derived(board.refetching);
    const error = $derived(board.error);
    const counts = $derived(board.counts);
    const sort = $derived(board.sort);
    const statusFilter = $derived(board.statusFilter);
    const categoryFilter = $derived(board.categoryFilter);
    const searchQuery = $derived(board.searchQuery);
    const focusedPostIndex = $derived(board.focusedIndex);
    const authed = $derived(session.signedIn);

    let showForm = $state(false);
    // Mobile toolbar: filter panel visibility (see mobile toolbar markup).
    let showFilters = $state(false);
    // Set true once the initial onMount load finishes; gates the effects below
    // so they don't fire before the first board fetch completes.
    let initialized = $state(false);

    $effect(() => {
        if (!initialized) return;
        const next = hrefWithSort(window.location.href, sort);
        if (next !== window.location.href) history.replaceState(history.state, '', next);
    });

    const sortOptions = SORT_OPTIONS.map((o) => ({ value: o.value, label: o.label }));

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

    // What the board was last loaded for. The effect below runs once right after
    // the first load; without this it would fetch the same page a second time.
    const loadKey = () => JSON.stringify([slug, statusFilter, categoryFilter, sort, searchQuery]);
    let loadedKey = '';

    onMount(async () => {
        await session.ready();
        // The key the load is issued for — not whatever the slug/filters have
        // become by the time it lands (a navigation mid-load must still reload).
        const key = loadKey();
        await board.load(slug);
        // A failed first load leaves the key empty so the effect retries once.
        loadedKey = board.error ? '' : key;
        initialized = true;
    });

    // Reload when the slug, a filter or the sort changes; typing in the search
    // box is debounced by the state module. Only these are read here — reading
    // `project`/`posts` would re-trigger the effect on our own state writes.
    $effect(() => {
        if (!initialized) return;
        const key = loadKey();
        if (key === loadedKey) return;
        loadedKey = key;
        return board.queueReload(slug);
    });

    /**
     * Board-scope keyboard-shortcut handler. Receives `rungu:shortcut`
     * events dispatched by +layout.svelte. Implements the board subset of
     * the registry (see $lib/shortcuts.ts).
     */
    function onShortcut(e: Event) {
        const { key } = (e as CustomEvent).detail as {
            key: string;
            scope: string;
            authRequired?: boolean;
        };

        // Mark the event handled (preventDefault) only when an action ran, so
        // the layout swallows the key only then.
        switch (key) {
            case '/': {
                const el = document.getElementById('board-search');
                if (el instanceof HTMLElement) {
                    el.focus();
                    e.preventDefault(); // don't type the '/' into the search box
                }
                break;
            }
            case 'c': {
                if (!authed) return;
                showForm = true;
                e.preventDefault(); // don't type the 'c' into the title field
                break;
            }
            case 'j': {
                if (!board.focusNext()) return;
                scrollFocusedIntoView();
                e.preventDefault();
                break;
            }
            case 'k': {
                if (!board.focusPrev()) return;
                scrollFocusedIntoView();
                e.preventDefault();
                break;
            }
            case 'Enter': {
                const post = board.focusedPost;
                if (!post) return;
                goto(`/board/${slug}/post/${post.id}`);
                e.preventDefault();
                break;
            }
            case 'v': {
                // Voting always requires auth. `authRequired` is irrelevant
                // here (there's no anonymous vote path), so just guard on auth.
                if (!authed) return;
                const post = board.focusedPost;
                if (!post) return;
                board.toggleVote(post);
                e.preventDefault();
                break;
            }
        }
    }

    function scrollFocusedIntoView() {
        if (focusedPostIndex < 0) return;
        // Defer to after DOM updates.
        queueMicrotask(() => {
            const el = document.querySelector(`[data-post-index="${board.focusedIndex}"]`);
            if (el instanceof HTMLElement) {
                el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }
        });
    }

    onMount(() => {
        window.addEventListener('rungu:shortcut', onShortcut as EventListener);
        return () => window.removeEventListener('rungu:shortcut', onShortcut as EventListener);
    });

    async function handleCreatePost(data: { title: string; description: string; category: PostCategory }) {
        await board.createPost(slug, data);
        showForm = false;
    }
</script>

<svelte:head>
    <title>{(project?.name ?? 'Board') + ' — ' + branding.value.brandName}</title>
</svelte:head>

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
            <!-- Desktop only: below lg the sticky toolbar carries this action,
                 so showing both put two New Post buttons on one screen. -->
            {#if authed}
                <Button class="hidden lg:inline-flex" onclick={() => (showForm = true)}>
                    <Plus class="size-4" aria-hidden="true" /> New Post
                </Button>
            {:else}
                <Button class="hidden lg:inline-flex" href={loginUrl}>Login to post</Button>
            {/if}
        </div>
    </div>

    <BoardTabs {slug} current="feedback" />

    <!-- Mobile action toolbar: sticky, replaces the desktop sidebar below lg.
         Navigation lives in the view tabs; this row carries New Post + Filters
         only. Tap targets >= 44px (mobile-ux HIG minimum). Sticks at top-0:
         the site nav is not sticky, so any offset left a gap that showed
         scrolled content above the toolbar. -->
    <div class="sticky top-0 z-20 -mx-4 mb-4 border-b bg-background/95 px-4 py-2 backdrop-blur supports-[backdrop-filter]:bg-background/80 lg:hidden">
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
                <Button size="sm" class="h-11 shrink-0" href={loginUrl}>Login to post</Button>
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
                    <h2 class="mb-1 text-xs font-semibold uppercase text-muted-foreground">Category</h2>
                    <div class="flex gap-1 overflow-x-auto pb-1" role="group" aria-label="Filter by category">
                        {#each categoryOptions as cat (cat.value)}
                            <button
                                onclick={() => board.toggleCategory(cat.value)}
                                aria-pressed={categoryFilter === cat.value}
                                class={cn(
                                    'min-h-11 shrink-0 whitespace-nowrap rounded-full border px-3 py-1 text-sm transition-colors',
                                    categoryFilter === cat.value
                                        ? 'border-primary bg-primary/10 font-medium text-primary-text'
                                        : 'border-input text-muted-foreground hover:bg-muted',
                                )}
                            >
                                {cat.label}{counts ? ` (${counts.by_category[cat.value] ?? 0})` : ''}
                            </button>
                        {/each}
                    </div>
                </div>
                <div>
                    <h2 class="mb-1 text-xs font-semibold uppercase text-muted-foreground">Status</h2>
                    <div class="flex gap-1 overflow-x-auto pb-1" role="group" aria-label="Filter by status">
                        {#each statusOptions as st (st.value)}
                            <button
                                onclick={() => board.toggleStatus(st.value)}
                                aria-pressed={statusFilter === st.value}
                                class={cn(
                                    'min-h-11 shrink-0 whitespace-nowrap rounded-full border px-3 py-1 text-sm capitalize transition-colors',
                                    statusFilter === st.value
                                        ? 'border-primary bg-primary/10 font-medium text-primary-text'
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

    <!-- New Post dialog (bits-ui): focus moves into the form, Tab is trapped
         inside, Esc / backdrop click close it, focus returns to the opener,
         and the page behind stops scrolling. Bottom sheet below sm. -->
    <Dialog.Root bind:open={showForm}>
        <Dialog.Portal>
            <Dialog.Overlay class="fixed inset-0 z-50 bg-background/80 backdrop-blur-sm" />
            <Dialog.Content
                class="fixed inset-x-0 bottom-0 z-50 max-h-[92vh] w-full overflow-y-auto rounded-t-xl border bg-background p-4 shadow-lg outline-none sm:inset-x-auto sm:bottom-auto sm:left-1/2 sm:top-1/2 sm:max-w-lg sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-xl"
                onOpenAutoFocus={(e) => {
                    // Land on the title field, not the close button.
                    e.preventDefault();
                    document.getElementById('post-form-title')?.focus();
                }}
            >
                <div class="mb-3 flex items-center justify-between">
                    <Dialog.Title class="text-base font-semibold">New Post</Dialog.Title>
                    <Dialog.Close>
                        {#snippet child({ props })}
                            <Button {...props} variant="ghost" size="icon-sm" class="max-sm:size-11" aria-label="Close">
                                <X class="size-4" aria-hidden="true" />
                            </Button>
                        {/snippet}
                    </Dialog.Close>
                </div>
                <PostForm {slug} onsubmit={handleCreatePost} showTitle={false} />
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>

    <div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
        <!-- Main -->
        <div class="min-w-0">
            <div class="mb-4 flex flex-wrap items-center gap-2">
                <div class="relative min-w-0 flex-1">
                    <Input
                        id="board-search"
                        bind:value={board.searchQuery}
                        type="search"
                        placeholder="Search..."
                        aria-label="Search posts"
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
                <SortTabs options={sortOptions} value={sort} onchange={(v) => (board.sort = v as Sort)} />
            </div>

            {#if statusFilter || categoryFilter}
                <div class="mb-4 flex flex-wrap gap-2">
                    {#if statusFilter}
                        <Button variant="secondary" size="xs" onclick={() => (board.statusFilter = '')}>
                            {statusOptions.find((s) => s.value === statusFilter)?.label} ✕
                        </Button>
                    {/if}
                    {#if categoryFilter}
                        <Button variant="secondary" size="xs" onclick={() => (board.categoryFilter = '')}>
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

            {#if posts.length > 0 && board.hasMore}
                <div class="mt-4 text-center">
                    <p class="mb-2 text-xs text-muted-foreground" aria-live="polite">
                        Showing {posts.length} of {total} posts
                    </p>
                    <Button variant="outline" size="sm" onclick={() => board.loadMore(slug)}>Show more</Button>
                </div>
            {:else if posts.length > 0 && !board.hasMore && total > PAGE_SIZE}
                <p class="mt-4 text-center text-xs text-muted-foreground">Showing all {posts.length} posts</p>
            {/if}

            {#if posts.length === 0}
                <div class="rounded-xl border border-dashed border-border py-12 text-center">
                    {#if board.hasFilters}
                        <p class="text-lg font-medium">No results</p>
                        <p class="mt-1 text-sm text-muted-foreground">No posts match your current filters.</p>
                        <Button
                            variant="outline"
                            size="sm"
                            class="mt-4"
                            onclick={() => board.clearFilters()}
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
                        <Button class="mt-4" href={loginUrl}>Login to post</Button>
                    {/if}
                </div>
            {/if}
        </div>

        <!-- Sidebar -->
        <!-- Sidebar: desktop-only content rail (mobile filters live in the
             sticky toolbar's chip sliders). Navigation lives in the view tabs;
             posting opens the dialog. -->
        <div class="hidden min-w-0 space-y-4 lg:block">
            <Card.Root class="gap-2 p-3">
                <h2 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Categories</h2>
                <div class="flex flex-col gap-1">
                    {#each categoryOptions as cat (cat.value)}
                        <button
                            onclick={() => board.toggleCategory(cat.value)}
                            class={cn(
                                'flex min-h-11 items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-sm transition-colors',
                                categoryFilter === cat.value
                                    ? 'bg-primary/10 font-medium text-primary-text'
                                    : 'text-muted-foreground hover:bg-muted',
                            )}
                        >
                            <span>{cat.label}</span>
                            <span class="text-xs tabular-nums text-muted-foreground">{counts?.by_category[cat.value] ?? 0}</span>
                        </button>
                    {/each}
                </div>
            </Card.Root>

            <Card.Root class="gap-2 p-3">
                <h2 class="mb-2 text-xs font-semibold uppercase text-muted-foreground">Status</h2>
                <div class="flex flex-col gap-1">
                    {#each statusOptions as st (st.value)}
                        <button
                            onclick={() => board.toggleStatus(st.value)}
                            class={cn(
                                'flex min-h-11 items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-sm capitalize transition-colors',
                                statusFilter === st.value
                                    ? 'bg-primary/10 font-medium text-primary-text'
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
