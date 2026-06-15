<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
    import type { PostCategory } from '$lib/api/types';

    let { category }: { category: PostCategory } = $props();

    const icons: Record<PostCategory, string> = {
        feedback: '💬',
        bug: '🐛',
        feature: '✨',
        question: '❓',
    };
</script>

<Badge variant="outline" class="gap-1">
    <span>{icons[category]}</span>
    <span class="capitalize">{category}</span>
</Badge>
