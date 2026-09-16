<script lang="ts">
	/**
	 * List/table toolbar: left slot holds search + filters, right slot holds
	 * refresh and primary actions. Keeps the filter → query → action row consistent.
	 */
	let {
		children,
		actions,
	}: {
		children?: any;
		actions?: any;
	} = $props();
</script>

<div class="toolbar">
	<div class="toolbar-left">
		{@render children?.()}
	</div>
	{#if actions}
		<div class="toolbar-right">
			{@render actions()}
		</div>
	{/if}
</div>

<style>
	.toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
		margin-bottom: 1.5rem;
	}

	.toolbar-left {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex: 1 1 auto;
		min-width: 0;
		flex-wrap: wrap;
	}

	.toolbar-right {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-shrink: 0;
	}
</style>
