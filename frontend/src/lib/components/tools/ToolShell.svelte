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
</style>
