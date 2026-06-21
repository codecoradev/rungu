<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { api } from '$lib/api/client';
    import type { Attachment } from '$lib/api/types';
    import { Button } from '$lib/components/ui/button';

    let {
        postId,
        canEdit = false,
    }: { postId: string; canEdit?: boolean } = $props();

    let attachments: Attachment[] = $state([]);
    let loading = $state(true);
    let error = $state('');
    let lightbox: Attachment | null = $state(null);

    async function loadAttachments() {
        try {
            loading = true;
            const res = await api.listAttachments(postId);
            attachments = res;
        } catch (e) {
            error = (e as Error).message;
        } finally {
            loading = false;
        }
    }

    async function handleUpload(event: Event) {
        const input = event.target as HTMLInputElement;
        const files = input.files;
        if (!files || files.length === 0) return;

        for (const file of Array.from(files)) {
            try {
                await api.uploadAttachment(postId, file);
            } catch (e) {
                error = (e as Error).message;
            }
        }
        input.value = '';
        await loadAttachments();
    }

    async function handleDelete(id: string) {
        if (!confirm('Delete this attachment?')) return;
        try {
            await api.deleteAttachment(id);
            await loadAttachments();
        } catch (e) {
            error = (e as Error).message;
        }
    }

    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') lightbox = null;
    }

    onMount(() => {
        loadAttachments();
        window.addEventListener('keydown', handleKeydown);
    });

    onDestroy(() => {
        window.removeEventListener('keydown', handleKeydown);
    });
</script>

{#if loading}
    <div class="py-2 text-muted-foreground text-sm">Loading attachments…</div>
{:else if attachments.length > 0}
    <div class="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4">
        {#each attachments as att (att.id)}
            <div class="group relative overflow-hidden rounded-lg border bg-muted/30">
                <button
                    type="button"
                    onclick={() => (lightbox = att)}
                    class="block w-full cursor-zoom-in"
                >
                    <img
                        src={att.url}
                        alt={att.filename}
                        class="aspect-square w-full object-cover transition hover:opacity-80"
                        loading="lazy"
                    />
                </button>
                {#if canEdit}
                    <button
                        type="button"
                        onclick={() => handleDelete(att.id)}
                        class="absolute top-1 right-1 rounded-md bg-background/80 p-1 opacity-0 transition group-hover:opacity-100 hover:bg-destructive hover:text-destructive-foreground"
                        title="Delete attachment"
                        aria-label="Delete attachment"
                    >
                        <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
                            <path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.5H3.75a.75.75 0 000 1.5h.45l.917 10.625A2.75 2.75 0 007.81 19h4.38a2.75 2.75 0 002.693-2.625L15.8 5.75h.45a.75.75 0 000-1.5H14v-.5A2.75 2.75 0 0011.25 1h-2.5z" clip-rule="evenodd" />
                        </svg>
                    </button>
                {/if}
            </div>
        {/each}
    </div>

    {#if canEdit}
        <label
            class="mt-2 flex cursor-pointer items-center gap-2 text-muted-foreground text-sm hover:text-foreground"
        >
            <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
                <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
            </svg>
            <span>Add image</span>
            <input
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                multiple
                onchange={handleUpload}
                class="hidden"
            />
        </label>
    {/if}
{:else if canEdit}
    <label
        class="flex cursor-pointer items-center gap-2 text-muted-foreground text-sm hover:text-foreground"
    >
        <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
            <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
        </svg>
        <span>Add image</span>
        <input
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            multiple
            onchange={handleUpload}
            class="hidden"
        />
    </label>
{/if}

{#if error}
    <p class="text-destructive text-sm">{error}</p>
{/if}

{#if lightbox}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4"
        onclick={() => (lightbox = null)}
        role="button"
        tabindex="-1"
    >
        <div class="relative max-h-full max-w-3xl" onclick={(e) => e.stopPropagation()} role="presentation">
            <img
                src={lightbox.url}
                alt={lightbox.filename}
                class="max-h-[85vh] rounded-lg object-contain"
            />
            <div class="mt-2 flex items-center justify-between text-muted-foreground text-sm text-white/80">
                <span>{lightbox.filename} ({formatSize(lightbox.size)})</span>
                <Button variant="ghost" size="sm" onclick={() => (lightbox = null)}>Close</Button>
            </div>
        </div>
    </div>
{/if}
