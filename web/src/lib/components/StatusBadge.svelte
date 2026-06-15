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
</script>

<Badge variant="secondary" class="capitalize">{labels[status]}</Badge>
