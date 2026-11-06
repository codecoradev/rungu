<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import type { PostCategory } from '$lib/api/types';
    import { cn } from '$lib/utils';
    import { api } from '$lib/api/client';
    import type { PostDetail } from '$lib/api/types';
    import ChevronUp from '@lucide/svelte/icons/chevron-up';

    let {
        slug,
        onsubmit,
        showTitle = true,
    }: {
        slug: string;
        onsubmit: (data: { title: string; description: string; category: PostCategory }) => Promise<void>;
        /** Hide the card title when the form is embedded in a dialog that already has one. */
        showTitle?: boolean;
    } = $props();

    let title = $state('');
    let description = $state('');
    let category = $state<PostCategory>('feedback');
    let loading = $state(false);
    let error = $state('');

    // As-you-type duplicate check: after a pause in typing, look up existing
    // posts that share title keywords, so the author can vote on one instead
    // of filing a duplicate. Best-effort: failures just show nothing.
    let similar = $state<PostDetail[]>([]);
    let similarFor = '';
    $effect(() => {
        const draft = title.trim();
        // The server matches on words of 4+ characters; skip until one exists.
        if (!draft.split(/\s+/).some((w) => w.length >= 4)) {
            similar = [];
            similarFor = '';
            return;
        }
        const timer = setTimeout(async () => {
            similarFor = draft;
            try {
                const found = await api.getSimilarByTitle(slug, draft);
                if (similarFor === draft) similar = found; // ignore stale replies
            } catch {
                if (similarFor === draft) similar = [];
            }
        }, 400);
        return () => clearTimeout(timer);
    });

    const categories: { value: PostCategory; label: string }[] = [
        { value: 'feedback', label: 'Feedback' },
        { value: 'bug', label: 'Bug' },
        { value: 'feature', label: 'Feature' },
        { value: 'question', label: 'Question' },
    ];

    async function handleSubmit(e: Event) {
        e.preventDefault();
        if (!title.trim()) {
            error = 'Title is required';
            return;
        }
        loading = true;
        error = '';
        try {
            await onsubmit({ title: title.trim(), description: description.trim(), category });
            title = '';
            description = '';
            category = 'feedback';
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to create post';
        } finally {
            loading = false;
        }
    }
</script>

{#snippet formBody()}
    <form onsubmit={handleSubmit} class="space-y-3" novalidate>
        {#if error}
            <p id="post-form-error" class="text-sm text-destructive" role="alert">{error}</p>
        {/if}

        <div>
            <Input
                id="post-form-title"
                bind:value={title}
                placeholder="What's your feedback?"
                maxlength={200}
                aria-label="Title"
                aria-invalid={error ? 'true' : undefined}
                aria-describedby={error ? 'post-form-error' : undefined}
            />
            <p class="mt-1 text-right text-xs text-muted-foreground" aria-live="polite">
                {title.length}/200
            </p>
            <div aria-live="polite">
                {#if similar.length > 0}
                    <div class="mt-1 rounded-lg border border-border bg-muted/40 p-3">
                        <p class="text-xs font-medium text-muted-foreground">
                            Similar posts already exist. Vote on one instead?
                        </p>
                        <ul class="mt-1 flex flex-col">
                            {#each similar as post (post.id)}
                                <li>
                                    <a
                                        href={`/board/${slug}/post/${post.id}`}
                                        class="flex min-h-11 items-center justify-between gap-3 rounded-md px-2 text-sm hover:bg-muted"
                                    >
                                        <span class="min-w-0 truncate" title={post.title}>{post.title}</span>
                                        <span class="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                                            <ChevronUp class="size-3.5" aria-hidden="true" />
                                            {post.vote_count}
                                            <span class="sr-only">votes</span>
                                        </span>
                                    </a>
                                </li>
                            {/each}
                        </ul>
                    </div>
                {/if}
            </div>
        </div>

        <div>
            <Textarea bind:value={description} placeholder="Add more details (optional)" rows={3} maxlength={4000} aria-label="Description" />
            {#if description.length > 3600}
                <p class="mt-1 text-right text-xs text-muted-foreground" aria-live="polite">
                    {description.length}/4000
                </p>
            {/if}
        </div>

        <!-- Category: 2x2 toggle buttons, one pressed at a time -->
        <div class="space-y-1.5">
            <span id="post-form-category" class="text-xs font-medium text-muted-foreground">Category</span>
            <div class="grid grid-cols-2 gap-2" role="group" aria-labelledby="post-form-category">
                {#each categories as cat (cat.value)}
                    <button
                        type="button"
                        aria-pressed={category === cat.value}
                        onclick={() => (category = cat.value)}
                        class={cn(
                            'flex items-center gap-2 rounded-lg border px-3 py-2 text-sm font-medium transition-colors',
                            category === cat.value
                                ? 'border-primary bg-primary/10 text-primary-text'
                                : 'border-border text-muted-foreground hover:bg-muted',
                        )}
                    >
                        <span>{cat.label}</span>
                    </button>
                {/each}
            </div>
        </div>

        <Button type="submit" class="w-full" disabled={loading}>
            {loading ? 'Posting...' : 'Submit'}
        </Button>
    </form>
{/snippet}

<!-- Standalone: a titled card. Embedded in a dialog (showTitle=false): just
     the form, so the dialog does not draw a card inside its own panel. -->
{#if showTitle}
    <Card.Root>
        <Card.Header>
            <Card.Title class="text-base">New Post</Card.Title>
        </Card.Header>
        <Card.Content>
            {@render formBody()}
        </Card.Content>
    </Card.Root>
{:else}
    {@render formBody()}
{/if}
