<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'default' | 'active' | 'inactive' | 'info' | 'success' | 'warning' | 'danger';
		label?: string;
		children?: Snippet;
	}

	let {
		variant = 'default',
		label = '',
		children
	}: Props = $props();
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

<style>
	.badge {
		display: inline-block;
		padding: 0.25rem 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border: 1px solid var(--black);
	}

	.badge-active {
		background: var(--accent);
		color: var(--white);
		border-color: var(--accent);
	}

	.badge-inactive {
		background: var(--gray-200);
		color: var(--gray-700);
	}

	.badge-info {
		background: var(--info);
		color: var(--white);
		border-color: var(--info);
	}

	.badge-success {
		background: var(--success);
		color: var(--white);
		border-color: var(--success);
	}

	.badge-warning {
		background: var(--warning);
		color: var(--white);
		border-color: var(--warning);
	}

	.badge-danger {
		background: var(--danger);
		color: var(--white);
		border-color: var(--danger);
	}
</style>
