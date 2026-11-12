<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { BadgeVariant } from '$lib/utils/tone';

	interface Props {
		variant?: BadgeVariant;
		label?: string;
		children?: Snippet;
	}

	let { variant = 'default', label = '', children }: Props = $props();
</script>

<span
	class="badge"
	class:badge-active={variant === 'active'}
	class:badge-inactive={variant === 'inactive'}
	class:badge-info={variant === 'info'}
	class:badge-success={variant === 'success'}
	class:badge-warning={variant === 'warning'}
	class:badge-danger={variant === 'danger'}
>
	{#if label}
		{label}
	{:else}
		{@render children?.()}
	{/if}
</span>
