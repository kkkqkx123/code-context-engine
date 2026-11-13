<script lang="ts">
	/**
	 * Connected-component (community) chips for the graph explorer.
	 * Clicking a community asks the page to load its members as an
	 * induced subgraph; grouping itself is derived state passed down.
	 */
	import SelectChip from '$lib/components/ui/SelectChip.svelte';

	interface Props {
		communities: [number, string[]][];
		onLoad: (ids: string[]) => void;
	}

	let { communities, onLoad }: Props = $props();
</script>

{#if communities.length > 0}
	<section class="panel-section" aria-label="Connected components">
		<h4 class="panel-title">Communities ({communities.length})</h4>
		<p class="panel-hint">
			Click a community to load its members as an induced subgraph.
		</p>
		<ul class="community-list">
			{#each communities as [index, ids] (index)}
				<li>
					<SelectChip
						onclick={() => onLoad(ids)}
						title="Load community #{index} ({ids.length} members)"
					>
						<span class="community-index">#{index}</span>
						<span class="community-size">{ids.length}</span>
						<span class="community-preview font-mono">
							{ids.slice(0, 3).join(', ')}{ids.length > 3 ? ' …' : ''}
						</span>
					</SelectChip>
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.community-list {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 0.5rem;
		margin: 0;
		padding: 0;
	}

	.community-list :global(.select-chip) {
		display: grid;
		grid-template-columns: auto auto 1fr;
	}

	.community-index {
		color: var(--black);
	}

	.community-size {
		color: var(--gray-500);
	}

	.community-preview {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
