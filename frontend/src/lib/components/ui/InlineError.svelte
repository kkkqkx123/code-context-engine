<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		message?: string;
		children?: Snippet;
	}

	let { message = '', children }: Props = $props();
</script>

<div class="inline-error">
	{#if message}
		{message}
	{:else}
		{@render children?.()}
	{/if}
</div>

<style>
	.inline-error {
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
	}
</style>
