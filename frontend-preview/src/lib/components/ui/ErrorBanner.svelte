<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		message?: string;
		children?: Snippet;
	}

	let { message = '', children }: Props = $props();
</script>

<div class="error-banner">
	{#if message}
		<span>{message}</span>
	{:else}
		{@render children?.()}
	{/if}
</div>

<style>
	.error-banner {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		background: var(--danger);
		color: var(--white);
		padding: 1rem;
		margin-bottom: 1.5rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.error-banner :global(button) {
		background: none;
		border: none;
		color: var(--white);
		cursor: pointer;
		font-size: 1.1rem;
		line-height: 1;
	}
</style>