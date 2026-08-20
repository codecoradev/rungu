<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
    import type { PostCategory } from '$lib/api/types';
    import MessageSquare from '@lucide/svelte/icons/message-square';
    import Bug from '@lucide/svelte/icons/bug';
    import Sparkles from '@lucide/svelte/icons/sparkles';
    import CircleHelp from '@lucide/svelte/icons/circle-help';

    let { category }: { category: PostCategory } = $props();

    const icons: Record<PostCategory, typeof MessageSquare> = {
        feedback: MessageSquare,
        bug: Bug,
        feature: Sparkles,
        question: CircleHelp,
    };

    const Icon = $derived(icons[category]);
</script>

<Badge variant="outline" class="gap-1">
    <Icon class="size-3" aria-hidden="true" />
    <span class="capitalize">{category}</span>
</Badge>
