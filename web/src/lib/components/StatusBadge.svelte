<script lang="ts">
    import { Badge } from '$lib/components/ui/badge';
    import type { PostStatus } from '$lib/api/types';

    let { status }: { status: PostStatus } = $props();

    const labels: Record<PostStatus, string> = {
        open: 'Open',
        planned: 'Planned',
        in_progress: 'In Progress',
        done: 'Done',
        declined: 'Declined',
    };

    // Soft-tinted pill per status — one consistent treatment instead of a mix of
    // badge variants. Colors come from the --color-status-* tokens (app.css).
    const badgeClass: Record<PostStatus, string> = {
        open: 'bg-status-open/10 text-status-open',
        planned: 'bg-status-planned/10 text-status-planned',
        in_progress: 'bg-status-in-progress/10 text-status-in-progress',
        done: 'bg-status-done/10 text-status-done',
        declined: 'bg-status-declined/10 text-status-declined',
    };

    const dotColors: Record<PostStatus, string> = {
        open: 'bg-status-open',
        planned: 'bg-status-planned',
        in_progress: 'bg-status-in-progress',
        done: 'bg-status-done',
        declined: 'bg-status-declined',
    };
</script>

<Badge variant="outline" class="gap-1.5 border-transparent {badgeClass[status]}">
    <span class="size-1.5 rounded-full {dotColors[status]}"></span>
    {labels[status]}
</Badge>
