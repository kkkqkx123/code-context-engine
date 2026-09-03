<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import type { Snippet } from 'svelte';

	interface Props extends HTMLButtonAttributes {
		variant?: 'primary' | 'secondary' | 'danger';
		size?: 'sm' | 'md' | 'lg';
		disabled?: boolean;
		loading?: boolean;
		children?: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		loading = false,
		class: className = undefined,
		children,
		...rest
	}: Props = $props();
</script>

<button
	class={['btn', className]}
	class:btn-secondary={variant === 'secondary'}
	class:btn-danger={variant === 'danger'}
	class:btn-sm={size === 'sm'}
	class:btn-lg={size === 'lg'}
	{disabled}
	{...rest}
>
	{#if loading}
		<span class="loading-indicator">...</span>
	{/if}
	{@render children?.()}
</button>

<style>
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem 1.75rem;
		background: var(--black);
		color: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border: none;
		cursor: pointer;
		transition: all 0.3s;
		text-decoration: none;
		width: fit-content;
	}

	.btn:hover:not(:disabled) {
		background: var(--accent);
		transform: translateX(4px);
	}

	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-secondary {
		background: var(--white);
		color: var(--black);
		border: 1px solid var(--black);
	}

	.btn-secondary:hover:not(:disabled) {
		background: var(--black);
		color: var(--white);
	}

	.btn-danger {
		background: var(--danger);
		color: var(--white);
	}

	.btn-danger:hover:not(:disabled) {
		background: var(--black);
	}

	.btn-sm {
		padding: 0.5rem 1rem;
		font-size: 0.7rem;
	}

	.btn-lg {
		padding: 1.25rem 2rem;
		font-size: 0.9rem;
	}

	.loading-indicator {
		animation: pulse 1s infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.5; }
	}
</style>
