<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		message?: string;
		hint?: string;
		fill?: boolean;
		padded?: boolean;
		card?: boolean;
		children?: Snippet;
	}

	let {
		message = '',
		hint = '',
		fill = false,
		padded = false,
		card = false,
		children,
	}: Props = $props();
</script>

<div
	class="empty-state"
	class:empty-state-fill={fill}
	class:empty-state-padded={padded}
	class:empty-state-card={card}
>
	{#if children}
		{@render children()}
	{:else}
		{#if message}
			<span>{message}</span>
		{/if}
		{#if hint}
			<span class="empty-hint">{hint}</span>
		{/if}
	{/if}
</div>

<style>
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		color: var(--gray-500);
		font-style: italic;
	}

	.empty-hint {
		font-size: 0.85em;
		color: var(--gray-400);
	}

	.empty-state-fill {
		flex: 1;
	}

	.empty-state-padded {
		padding: 3rem;
	}

	.empty-state-card {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1rem;
		padding: 1.5rem;
		border: 1px dashed var(--gray-300);
		background: var(--gray-100);
		color: var(--gray-600);
		font-style: normal;
	}
</style>
