<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'info' | 'warning';
		message?: string;
		label?: string;
		children?: Snippet;
		action?: Snippet;
	}

	let {
		variant = 'info',
		message = '',
		label = 'Notice',
		children,
		action,
	}: Props = $props();
</script>

<div
	class="notice"
	class:notice-info={variant === 'info'}
	class:notice-warning={variant === 'warning'}
	role="status"
	aria-label={label}
>
	<div class="notice-content">
		{#if message}
			<span>{message}</span>
		{:else if children}
			{@render children()}
		{/if}
	</div>
	{#if action}
		<div class="notice-action">
			{@render action()}
		</div>
	{/if}
</div>

<style>
	.notice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		margin-bottom: 1rem;
		border-left: 4px solid var(--info);
		background: var(--info-bg);
		color: var(--gray-700);
		font-size: 0.85rem;
	}

	.notice-warning {
		border-left-color: var(--warning);
		background: var(--warning-bg);
	}

	.notice-content {
		min-width: 0;
		flex: 1;
	}

	.notice-action {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
</style>
