<script lang="ts" module>
	import { cn, type WithElementRef } from '$lib/utils.js';
	import type { HTMLTextareaAttributes } from 'svelte/elements';
	import { tv } from 'tailwind-variants';

	export const textareaVariants = tv({
		base: 'border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 flex field-sizing-content min-h-16 w-full rounded-md border px-3 py-2 text-sm shadow-xs transition-[color,box-shadow] outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50',
	});

	export type TextareaProps = WithElementRef<HTMLTextareaAttributes>;
</script>

<script lang="ts">
	let {
		class: className,
		ref = $bindable(null),
		...restProps
	}: TextareaProps = $props();
</script>

<textarea
	bind:this={ref}
	data-slot="textarea"
	class={cn(textareaVariants(), className)}
	{...restProps}
></textarea>
