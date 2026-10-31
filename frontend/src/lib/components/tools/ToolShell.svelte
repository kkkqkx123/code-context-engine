<script lang="ts">
	import type { Snippet } from 'svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';

	interface Props {
		error?: string | null;
		children?: Snippet;
	}

	let { error = null, children }: Props = $props();
</script>

<div class="tool-shell">
	{#if error}
		<InlineError message={error} />
	{/if}
	{@render children?.()}
</div>

<style>
	.tool-shell {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		width: 100%;
	}

	.tool-shell :global(.tool-form-row) {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 0.75rem;
	}

	.tool-shell :global(.tool-actions) {
		margin-top: 1rem;
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 1rem;
	}
</style>
