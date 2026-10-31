<script lang="ts">
	import type { Snippet } from 'svelte';

	let { children }: { children?: Snippet } = $props();
</script>

<div class="tool-actions">
	{@render children?.()}
</div>

<style>
	.tool-actions {
		margin-top: 1rem;
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 1rem;
	}
</style>
