<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		message?: string;
		children?: Snippet;
	}

	let { message = 'Loading...', children }: Props = $props();
</script>

<div class="loading-state">
	{#if message}
		{message}
	{:else}
		{@render children?.()}
	{/if}
</div>

<style>
	.loading-state {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}
</style>
