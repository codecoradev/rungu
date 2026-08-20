<script lang="ts">
    import { onMount } from 'svelte';
    import { api, ApiError } from '$lib/api/client';
    import type { Project, CurrentUser, PostDetail, PostStatus, Webhook, WebhookDelivery, WebhookTestResult, ProjectStats } from '$lib/api/types';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import { Skeleton } from '$lib/components/ui/skeleton';
    import { timeAgo } from '$lib/utils';
    import { toastSuccess } from '$lib/toast.svelte';
    import StatusBadge from '$lib/components/StatusBadge.svelte';

    let projects = $state<Project[]>([]);
    let user = $state<CurrentUser | null>(null);
    let loading = $state(true);
    let error = $state('');

    // Tabs: 'projects' | 'moderation' | 'webhooks'
    let tab = $state<'projects' | 'moderation' | 'webhooks'>('projects');

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

    // Delete confirm state (two-step with stats preview)
    let deleting = $state<string | null>(null);
    let deleteStats = $state<ProjectStats | null>(null);
    let deleteLoading = $state(false);

    // Moderation queue
    let queue = $state<PostDetail[]>([]);
    let queueTotal = $state(0);
    let queueStatus = $state('');
    let queueProject = $state('');
    let queueLoading = $state(false);

    // Webhooks
    let selectedProject = $state('');
    let webhooks = $state<Webhook[]>([]);
    let webhooksLoading = $state(false);
    let newWebhookUrl = $state('');
    let webhookTesting = $state<string | null>(null);
    let testResult = $state<Record<string, { state: 'loading' } | { state: 'done'; result: WebhookTestResult }>>({});
    let deliveries = $state<Record<string, WebhookDelivery[]>>({});

    const isAdmin = $derived(user?.role === 'admin');

    // Success feedback toast (#153) — auto-dismiss, never overlaps the error banner
    function notify(msg: string) {
        toastSuccess(msg);
    }

    async function loadData() {
        loading = true;
        try {
            const currentUser = await api.getCurrentUser();
            user = currentUser;
            if (currentUser.role !== 'admin') {
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

    // ── Projects ──────────────────────────────────────────────────────

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
                description: editDesc,
            });
            projects = projects.map((p) => (p.slug === slug ? updated : p));
            editing = null;
            notify('Project saved ✓');
        } catch {
            error = 'Failed to update project';
        }
    }

    async function askDelete(slug: string) {
        deleting = slug;
        deleteStats = null;
        deleteLoading = true;
        try {
            deleteStats = await api.adminProjectStats(slug);
        } catch {
            deleteStats = null;
        } finally {
            deleteLoading = false;
        }
    }

    async function confirmDelete(slug: string) {
        try {
            await api.deleteProject(slug);
            projects = projects.filter((p) => p.slug !== slug);
            deleting = null;
            deleteStats = null;
            notify('Project deleted ✓');
        } catch {
            error = 'Failed to delete project';
            deleting = null;
        }
    }

    // ── Moderation ────────────────────────────────────────────────────

    async function loadQueue() {
        queueLoading = true;
        try {
            const res = await api.adminListAllPosts({
                status: queueStatus || undefined,
                project: queueProject || undefined,
                per_page: 50,
            });
            queue = res.data;
            queueTotal = res.pagination.total;
        } catch {
            error = 'Failed to load queue';
        } finally {
            queueLoading = false;
        }
    }

    async function moderate(post: PostDetail, status: PostStatus) {
        try {
            await api.updatePostStatus(post.id, status);
            if (queueStatus) {
                // Post leaves the filtered list only when its new status no longer matches the filter
                const matches = queueStatus === status;
                if (!matches) {
                    queue = queue.filter((p) => p.id !== post.id);
                    queueTotal = Math.max(0, queueTotal - 1);
                } else {
                    post.status = status as PostDetail['status'];
                }
            } else {
                post.status = status as PostDetail['status'];
            }
            notify(`Post marked ${status.replace('_', ' ')} ✓`);
        } catch {
            error = 'Failed to update post';
        }
    }

    // ── Webhooks ──────────────────────────────────────────────────────

    async function loadWebhooks() {
        if (!selectedProject) return;
        webhooksLoading = true;
        try {
            webhooks = await api.listWebhooks(selectedProject);
            testResult = {};
            deliveries = {};
            deletingWebhook = null;
        } catch {
            error = 'Failed to load webhooks';
        } finally {
            webhooksLoading = false;
        }
    }

    async function addWebhook(e: Event) {
        e.preventDefault();
        if (!newWebhookUrl.trim() || !selectedProject) return;
        try {
            const wh = await api.createWebhook(selectedProject, { url: newWebhookUrl.trim(), events: 'post.created,post.status_changed,comment.created' });
            webhooks = [...webhooks, wh];
            newWebhookUrl = '';
        } catch (e) {
            error = e instanceof ApiError ? e.message : 'Failed to create webhook';
        }
    }

    async function toggleWebhook(wh: Webhook) {
        try {
            const updated = await api.updateWebhook(selectedProject, wh.id, { is_active: !wh.is_active });
            webhooks = webhooks.map((w) => (w.id === wh.id ? updated : w));
            notify(updated.is_active ? 'Webhook activated ✓' : 'Webhook paused ✓');
        } catch {
            error = 'Failed to update webhook';
        }
    }

    let deletingWebhook = $state<string | null>(null);

    async function removeWebhook(wh: Webhook) {
        try {
            await api.deleteWebhook(selectedProject, wh.id);
            webhooks = webhooks.filter((w) => w.id !== wh.id);
            deletingWebhook = null;
            notify('Webhook deleted ✓');
        } catch {
            error = 'Failed to delete webhook';
        }
    }

    async function runTest(wh: Webhook) {
        webhookTesting = wh.id;
        testResult = { ...testResult, [wh.id]: { state: 'loading' } };
        try {
            const res = await api.testWebhook(selectedProject, wh.id);
            testResult = { ...testResult, [wh.id]: { state: 'done', result: res } };
            const dl = await api.listWebhookDeliveries(selectedProject, wh.id);
            deliveries = { ...deliveries, [wh.id]: dl };
        } catch (e) {
            testResult = {
                ...testResult,
                [wh.id]: { state: 'done', result: { ok: false, error: e instanceof ApiError ? e.message : 'Request failed' } },
            };
        } finally {
            webhookTesting = null;
        }
    }

    async function toggleDeliveries(wh: Webhook) {
        if (deliveries[wh.id]) {
            const { [wh.id]: _, ...rest } = deliveries;
            deliveries = rest;
        } else {
            try {
                const dl = await api.listWebhookDeliveries(selectedProject, wh.id);
                deliveries = { ...deliveries, [wh.id]: dl };
            } catch {
                error = 'Failed to load deliveries';
            }
        }
    }

    const tabs = [
        { id: 'projects', label: 'Projects' },
        { id: 'moderation', label: 'Moderation' },
        { id: 'webhooks', label: 'Webhooks' },
    ] as const;
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
        <p class="mt-1 text-sm text-muted-foreground">Manage projects, moderate posts, and configure webhooks</p>
    </div>

    {#if error}
        <p class="mb-4 text-sm text-destructive">{error}</p>
    {/if}


    <!-- Tabs -->
    <div class="mb-6 flex gap-1 border-b" role="tablist">
        {#each tabs as t (t.id)}
            <button
                role="tab"
                aria-selected={tab === t.id}
                class="rounded-t-md px-4 py-2 text-sm font-medium transition-colors {tab === t.id
                    ? 'border-b-2 border-primary text-foreground'
                    : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => {
                    tab = t.id;
                    error = '';
                    if (t.id === 'moderation' && queue.length === 0) loadQueue();
                    if (t.id === 'webhooks' && selectedProject === '' && projects.length > 0) {
                        selectedProject = projects[0].slug;
                        loadWebhooks();
                    }
                }}
            >
                {t.label}
            </button>
        {/each}
    </div>

    <!-- ═══ PROJECTS TAB ═══ -->
    {#if tab === 'projects'}
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
                        <div class="space-y-1.5">
                            <label for="new-project-name" class="text-sm font-medium">Project name</label>
                            <Input id="new-project-name" bind:value={newName} placeholder="Project name" required oninput={() => { if (!newSlug) newSlug = slugify(newName); }} />
                        </div>
                        <div class="space-y-1.5">
                            <label for="new-project-slug" class="text-sm font-medium">Slug</label>
                            <Input id="new-project-slug" bind:value={newSlug} placeholder="auto from name" />
                        </div>
                    </div>
                    <div class="space-y-1.5">
                        <label for="new-project-desc" class="text-sm font-medium">Description <span class="font-normal text-muted-foreground">(optional)</span></label>
                        <Textarea id="new-project-desc" bind:value={newDesc} placeholder="What is this board for?" rows={2} />
                    </div>
                    <Button type="submit" disabled={creating || !newName.trim()}>
                        {creating ? 'Creating...' : 'Create Project'}
                    </Button>
                </form>
            </Card.Content>
        </Card.Root>

        <div class="space-y-3">
            <h2 class="text-sm font-semibold text-muted-foreground">Projects ({projects.length})</h2>

            {#each projects as project (project.id)}
                <Card.Root>
                    {#if editing === project.slug}
                        <Card.Content class="space-y-3 pt-6">
                            <div class="space-y-1.5">
                                <label for={`edit-name-${project.slug}`} class="text-sm font-medium">Name</label>
                                <Input id={`edit-name-${project.slug}`} bind:value={editName} placeholder="Name" />
                            </div>
                            <div class="space-y-1.5">
                                <label for={`edit-desc-${project.slug}`} class="text-sm font-medium">Description</label>
                                <Textarea id={`edit-desc-${project.slug}`} bind:value={editDesc} placeholder="Description" rows={2} />
                            </div>
                            <div class="flex gap-2">
                                <Button size="sm" onclick={() => saveEdit(project.slug)}>Save</Button>
                                <Button variant="outline" size="sm" onclick={() => (editing = null)}>Cancel</Button>
                            </div>
                        </Card.Content>
                    {:else if deleting === project.slug}
                        <Card.Content class="space-y-3 pt-6">
                            <p class="font-medium text-destructive">Delete "{project.name}"?</p>
                            {#if deleteLoading}
                                <p class="text-sm text-muted-foreground">Loading stats…</p>
                            {:else if deleteStats}
                                <p class="text-sm text-muted-foreground">
                                    This will permanently delete
                                    <strong>{deleteStats.total_posts} posts</strong>,
                                    <strong>{deleteStats.total_votes} votes</strong>, and
                                    <strong>{deleteStats.total_comments} comments</strong>.
                                </p>
                            {:else}
                                <p class="text-sm text-muted-foreground">This will cascade-delete all posts, votes, and comments.</p>
                            {/if}
                            <div class="flex gap-2">
                                <Button variant="destructive" size="sm" onclick={() => confirmDelete(project.slug)}>Delete permanently</Button>
                                <Button variant="outline" size="sm" onclick={() => { deleting = null; deleteStats = null; }}>Cancel</Button>
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
                            <div class="flex shrink-0 items-center gap-1">
                                <Button variant="ghost" size="sm" class="h-11 min-w-11" href={`/board/${project.slug}`} aria-label="View board">View</Button>
                                <Button variant="ghost" size="sm" class="h-11 min-w-11" onclick={() => startEdit(project)} aria-label="Edit project">Edit</Button>
                                <Button variant="ghost" size="sm" class="h-11 min-w-11 text-destructive" onclick={() => askDelete(project.slug)} aria-label="Delete project">Delete</Button>
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

    <!-- ═══ MODERATION TAB ═══ -->
    {#if tab === 'moderation'}
        <div class="mb-4 flex flex-wrap items-center gap-2">
            <select bind:value={queueStatus} onchange={loadQueue} class="h-9 rounded-md border bg-background px-3 text-sm" aria-label="Filter by status">
                <option value="">All statuses</option>
                <option value="open">Open</option>
                <option value="planned">Planned</option>
                <option value="in_progress">In progress</option>
                <option value="done">Done</option>
                <option value="declined">Declined</option>
            </select>
            <select bind:value={queueProject} onchange={loadQueue} class="h-9 rounded-md border bg-background px-3 text-sm" aria-label="Filter by project">
                <option value="">All projects</option>
                {#each projects as p (p.id)}
                    <option value={p.slug}>{p.name}</option>
                {/each}
            </select>
            <Button variant="outline" size="sm" onclick={loadQueue}>Refresh</Button>
            <span class="ml-auto text-xs text-muted-foreground">{queueTotal} post{queueTotal === 1 ? '' : 's'}</span>
        </div>

        {#if queueLoading}
            <Skeleton class="h-40 w-full" />
        {:else}
            <div class="space-y-3">
                {#each queue as post (post.id)}
                    <Card.Root>
                        <Card.Content class="flex flex-col gap-3 pt-6 sm:flex-row sm:items-start sm:justify-between">
                            <div class="min-w-0 flex-1">
                                <div class="flex flex-wrap items-center gap-2">
                                    <StatusBadge status={post.status} />
                                    <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">{post.category}</span>
                                    <span class="text-xs text-muted-foreground">▲ {post.vote_count} · {post.comment_count} comments · {timeAgo(post.created_at)}</span>
                                </div>
                                <p class="mt-2 font-medium">{post.title}</p>
                                <p class="mt-1 text-xs text-muted-foreground">by {post.creator?.name ?? post.created_by}</p>
                            </div>
                            <div class="flex shrink-0 flex-wrap gap-1">
                                <Button variant="outline" size="sm" onclick={() => moderate(post, 'planned')}>Plan</Button>
                                <Button variant="outline" size="sm" onclick={() => moderate(post, 'in_progress')}>Start</Button>
                                <Button variant="outline" size="sm" onclick={() => moderate(post, 'done')}>Done</Button>
                                <Button variant="outline" size="sm" class="text-destructive" onclick={() => moderate(post, 'declined')}>Decline</Button>
                            </div>
                        </Card.Content>
                    </Card.Root>
                {/each}
                {#if queue.length === 0}
                    <p class="py-8 text-center text-sm text-muted-foreground">No posts match this filter.</p>
                {/if}
            </div>
        {/if}
    {/if}

    <!-- ═══ WEBHOOKS TAB ═══ -->
    {#if tab === 'webhooks'}
        {#if projects.length === 0}
            <p class="py-8 text-center text-sm text-muted-foreground">Create a project first.</p>
        {:else}
            <div class="mb-4 flex flex-wrap items-center gap-2">
                <select bind:value={selectedProject} onchange={loadWebhooks} class="h-9 rounded-md border bg-background px-3 text-sm" aria-label="Project">
                    {#each projects as p (p.id)}
                        <option value={p.slug}>{p.name}</option>
                    {/each}
                </select>
            </div>

            <Card.Root class="mb-6">
                <Card.Header>
                    <Card.Title class="text-base">Add Webhook</Card.Title>
                </Card.Header>
                <Card.Content>
                    <form onsubmit={addWebhook} class="flex flex-col gap-3 sm:flex-row">
                        <div class="flex-1 space-y-1.5">
                            <label for="new-webhook-url" class="text-sm font-medium">Webhook URL</label>
                            <Input id="new-webhook-url" bind:value={newWebhookUrl} placeholder="https://example.com/webhook" required type="url" class="flex-1" />
                        </div>
                        <Button type="submit" disabled={!newWebhookUrl.trim()} class="sm:mt-6">Add</Button>
                    </form>
                    <p class="mt-2 text-xs text-muted-foreground">
                        Events: post.created, post.status_changed, comment.created. Signed with HMAC-SHA256 (X-Rungu-Signature).
                    </p>
                </Card.Content>
            </Card.Root>

            {#if webhooksLoading}
                <Skeleton class="h-40 w-full" />
            {:else}
                <div class="space-y-3">
                    {#each webhooks as wh (wh.id)}
                        <Card.Root>
                            <Card.Content class="space-y-3 pt-6">
                                <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                                    <div class="min-w-0 flex-1">
                                        <p class="truncate text-sm font-medium">{wh.url}</p>
                                        <p class="mt-1 text-xs text-muted-foreground">
                                            {wh.is_active ? 'Active' : 'Paused'} · added {timeAgo(wh.created_at)}
                                        </p>
                                    </div>
                                    <div class="flex shrink-0 flex-wrap gap-1">
                                        <Button variant="outline" size="sm" disabled={webhookTesting === wh.id || !wh.is_active} onclick={() => runTest(wh)}>
                                            {webhookTesting === wh.id ? 'Testing…' : 'Test'}
                                        </Button>
                                        <Button variant="outline" size="sm" onclick={() => toggleWebhook(wh)}>
                                            {wh.is_active ? 'Pause' : 'Activate'}
                                        </Button>
                                        <Button variant="outline" size="sm" onclick={() => toggleDeliveries(wh)}>
                                            {deliveries[wh.id] ? 'Hide log' : 'Log'}
                                        </Button>
                                        {#if deletingWebhook === wh.id}
                                            <Button variant="destructive" size="sm" onclick={() => removeWebhook(wh)}>Confirm delete</Button>
                                            <Button variant="outline" size="sm" onclick={() => (deletingWebhook = null)}>Cancel</Button>
                                        {:else}
                                            <Button variant="outline" size="sm" class="text-destructive" onclick={() => (deletingWebhook = wh.id)}>Delete</Button>
                                        {/if}
                                    </div>
                                </div>

                                {#if testResult[wh.id]}
                                    {@const tr = testResult[wh.id]}
                                    {#if tr.state === 'loading'}
                                        <p class="text-xs text-muted-foreground">Sending test event…</p>
                                    {:else if tr.result.ok}
                                        <p class="text-xs text-success">✓ Test delivered (HTTP {tr.result.status})</p>
                                    {:else}
                                        <p class="text-xs text-destructive">✗ {tr.result.error ?? 'Failed'}{tr.result.status ? ` (HTTP ${tr.result.status})` : ''}</p>
                                    {/if}
                                {/if}

                                {#if deliveries[wh.id]}
                                    <div class="rounded-md border p-2">
                                        {#if deliveries[wh.id].length === 0}
                                            <p class="text-xs text-muted-foreground">No deliveries yet.</p>
                                        {:else}
                                            <ul class="divide-y text-xs">
                                                {#each deliveries[wh.id].slice(0, 10) as d (d.id)}
                                                    <li class="flex items-center justify-between gap-2 py-1.5">
                                                        <span class="text-muted-foreground">{d.event_type} · {timeAgo(d.created_at)}</span>
                                                        <span class={d.success ? 'text-success' : 'text-destructive'}>
                                                            {d.success ? '✓' : '✗'} {d.status_code ?? 'err'}{d.attempts > 1 ? ` (${d.attempts}x)` : ''}
                                                        </span>
                                                    </li>
                                                {/each}
                                            </ul>
                                        {/if}
                                    </div>
                                {/if}
                            </Card.Content>
                        </Card.Root>
                    {/each}
                    {#if webhooks.length === 0}
                        <p class="py-8 text-center text-sm text-muted-foreground">No webhooks for this project. Add one above.</p>
                    {/if}
                </div>
            {/if}
        {/if}
    {/if}
{/if}
