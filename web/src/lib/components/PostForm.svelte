<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';
    import { Textarea } from '$lib/components/ui/textarea';
    import * as Card from '$lib/components/ui/card';
    import type { PostCategory } from '$lib/api/types';
    import { cn } from '$lib/utils';

    let {
        slug,
        onsubmit,
    }: {
        slug: string;
        onsubmit: (data: { title: string; description: string; category: PostCategory }) => Promise<void>;
    } = $props();

    let title = $state('');
    let description = $state('');
    let category = $state<PostCategory>('feedback');
    let loading = $state(false);
    let error = $state('');

    const categories: { value: PostCategory; label: string; icon: string }[] = [
        { value: 'feedback', label: 'Feedback', icon: '💬' },
        { value: 'bug', label: 'Bug', icon: '🐛' },
        { value: 'feature', label: 'Feature', icon: '✨' },
        { value: 'question', label: 'Question', icon: '❓' },
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

<Card.Root>
    <Card.Header>
        <Card.Title class="text-base">New Post</Card.Title>
    </Card.Header>
    <Card.Content>
        <form onsubmit={handleSubmit} class="space-y-3">
            {#if error}
                <p class="text-sm text-destructive">{error}</p>
            {/if}

            <Input bind:value={title} placeholder="What's your feedback?" maxlength={200} />

            <Textarea bind:value={description} placeholder="Add more details (optional)" rows={3} />

            <!-- Category: vertical list, no overflow -->
            <div class="space-y-1.5">
                <span class="text-xs font-medium text-muted-foreground">Category</span>
                <div class="grid grid-cols-2 gap-2">
                    {#each categories as cat (cat.value)}
                        <button
                            type="button"
                            onclick={() => (category = cat.value)}
                            class={cn(
                                'flex items-center gap-2 rounded-lg border px-3 py-2 text-sm font-medium transition-colors',
                                category === cat.value
                                    ? 'border-primary bg-primary/10 text-primary'
                                    : 'border-border text-muted-foreground hover:bg-muted',
                            )}
                        >
                            <span>{cat.icon}</span>
                            <span>{cat.label}</span>
                        </button>
                    {/each}
                </div>
            </div>

            <Button type="submit" class="w-full" disabled={loading}>
                {loading ? 'Posting...' : 'Submit'}
            </Button>
        </form>
    </Card.Content>
</Card.Root>
