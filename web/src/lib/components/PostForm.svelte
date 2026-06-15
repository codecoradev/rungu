<script lang="ts">
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

    const categories: { value: PostCategory; label: string }[] = [
        { value: 'feedback', label: '💬 Feedback' },
        { value: 'bug', label: '🐛 Bug' },
        { value: 'feature', label: '✨ Feature' },
        { value: 'question', label: '❓ Question' },
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

<form onsubmit={handleSubmit} class="rounded-xl border border-gray-200 bg-white p-4">
    <h3 class="mb-3 font-semibold text-gray-900">New Post</h3>

    {#if error}
        <div class="mb-3 rounded-lg bg-red-50 px-3 py-2 text-sm text-red-600">{error}</div>
    {/if}

    <div class="space-y-3">
        <input
            bind:value={title}
            type="text"
            placeholder="What's your feedback?"
            maxlength="200"
            class="w-full rounded-lg border border-gray-200 px-3 py-2 text-sm outline-none focus:border-brand-400 focus:ring-1 focus:ring-brand-400"
        />

        <textarea
            bind:value={description}
            placeholder="Add more details (optional)"
            rows="3"
            class="w-full resize-none rounded-lg border border-gray-200 px-3 py-2 text-sm outline-none focus:border-brand-400 focus:ring-1 focus:ring-brand-400"
        ></textarea>

        <div class="flex items-center gap-2">
            {#each categories as cat (cat.value)}
                <button
                    type="button"
                    onclick={() => (category = cat.value)}
                    class={cn(
                        'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
                        category === cat.value
                            ? 'bg-brand-50 text-brand-700'
                            : 'bg-gray-50 text-gray-500 hover:bg-gray-100',
                    )}
                >
                    {cat.label}
                </button>
            {/each}
        </div>

        <button
            type="submit"
            disabled={loading}
            class="w-full rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
        >
            {loading ? 'Posting...' : 'Submit'}
        </button>
    </div>
</form>
