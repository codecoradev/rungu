<script lang="ts">
    import type { PostStatus } from '$lib/api/types';
    import { cn } from '$lib/utils';

    let { status }: { status: PostStatus } = $props();

    const styles: Record<PostStatus, string> = {
        open: 'bg-blue-100 text-blue-700',
        planned: 'bg-purple-100 text-purple-700',
        in_progress: 'bg-yellow-100 text-yellow-700',
        done: 'bg-green-100 text-green-700',
        declined: 'bg-red-100 text-red-700',
    };

    const labels: Record<PostStatus, string> = {
        open: 'Open',
        planned: 'Planned',
        in_progress: 'In Progress',
        done: 'Done',
        declined: 'Declined',
    };
</script>

<span
    class={cn('inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium', styles[status])}
>
    {labels[status]}
</span>
