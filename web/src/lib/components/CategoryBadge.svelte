<script lang="ts">
    import type { PostCategory } from '$lib/api/types';
    import { cn } from '$lib/utils';

    let { category }: { category: PostCategory } = $props();

    const styles: Record<PostCategory, string> = {
        feedback: 'bg-gray-100 text-gray-600',
        bug: 'bg-red-50 text-red-600',
        feature: 'bg-blue-50 text-blue-600',
        question: 'bg-green-50 text-green-600',
    };

    const icons: Record<PostCategory, string> = {
        feedback: '💬',
        bug: '🐛',
        feature: '✨',
        question: '❓',
    };
</script>

<span
    class={cn('inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-medium', styles[category])}
>
    <span>{icons[category]}</span>
    <span class="capitalize">{category}</span>
</span>
