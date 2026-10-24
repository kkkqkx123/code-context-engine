<script lang="ts">
	/**
	 * Connected-component (community) chips for the graph explorer.
	 * Clicking a community asks the page to load its members as an
	 * induced subgraph; grouping itself is derived state passed down.
	 */
	interface Props {
		communities: [number, string[]][];
		onLoad: (ids: string[]) => void;
	}

	let { communities, onLoad }: Props = $props();
</script>

{#if communities.length > 0}
	<section class="communities" aria-label="Connected components">
		<h4 class="communities-title">Communities ({communities.length})</h4>
		<p class="communities-hint">
			Click a community to load its members as an induced subgraph.
		</p>
		<ul class="community-list">
			{#each communities as [index, ids] (index)}
				<li>
					<button
						type="button"
						class="community-btn"
						onclick={() => onLoad(ids)}
					>
						<span class="community-index">#{index}</span>
						<span class="community-size">{ids.length}</span>
						<span class="community-preview mono">
							{ids.slice(0, 3).join(', ')}{ids.length > 3 ? ' …' : ''}
						</span>
					</button>
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.communities {
		margin-top: 1rem;
		padding: 1rem;
		border: 1px solid var(--black);
	}

	.communities-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
		margin: 0 0 0.25rem;
	}

	.communities-hint {
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: 0 0 0.75rem;
	}

	.community-list {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 0.4rem;
		margin: 0;
		padding: 0;
	}

	.community-btn {
		display: grid;
		grid-template-columns: auto auto 1fr;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.35rem 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		text-align: left;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
		transition: all 0.15s;
	}

	.community-btn:hover {
		border-color: var(--black);
		background: var(--gray-100);
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

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}
</style>
