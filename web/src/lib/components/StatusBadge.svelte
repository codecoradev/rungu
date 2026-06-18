<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
    import { statusColor } from '$lib/utils';
    import type { PostStatus } from '$lib/api/types';

    let { status }: { status: PostStatus } = $props();

    const labels: Record<PostStatus, string> = {
        open: 'Open',
        planned: 'Planned',
        in_progress: 'In Progress',
        done: 'Done',
        declined: 'Declined',
    };

    // Color dots for visual differentiation beyond badge variant
    const dotColors: Record<PostStatus, string> = {
        open: 'bg-blue-500',
        planned: 'bg-violet-500',
        in_progress: 'bg-amber-500',
        done: 'bg-emerald-500',
        declined: 'bg-red-500',
    };
</script>

<Badge variant={statusColor(status)} class="gap-1.5">
    <span class="size-1.5 rounded-full {dotColors[status]}"></span>
    {labels[status]}
</Badge>
