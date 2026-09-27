<script lang="ts">
	import type { CallChainNode } from '$lib/api/search';
	import EntityGraphView from '$lib/components/graph/EntityGraphView.svelte';
	import { callChainToElements } from '$lib/utils/entity-graph';

	interface Props {
		nodes?: CallChainNode[];
		focusId?: string | null;
		onNavigate?: (id: string) => void;
	}

	let { nodes = [], focusId = null, onNavigate = () => {} }: Props = $props();

	let elements = $derived(callChainToElements(nodes));
	let effectiveFocusId = $derived(focusId ?? nodes[0]?.function_id ?? null);
	let viewMode = $state<'graph' | 'list'>('graph');

	function handleItemKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onNavigate(id);
		}
	}
</script>

<div class="call-graph-container">
	{#if nodes.length === 0}
		<div class="empty-state">
			<p>No call graph data available</p>
		</div>
	{:else}
		<div class="view-toggle" role="tablist" aria-label="Call graph view mode">
			<button
				type="button"
				class="toggle-btn"
				class:active={viewMode === 'graph'}
				onclick={() => (viewMode = 'graph')}
			>
				Graph
			</button>
			<button
				type="button"
				class="toggle-btn"
				class:active={viewMode === 'list'}
				onclick={() => (viewMode = 'list')}
			>
				List
			</button>
		</div>

		{#if viewMode === 'graph'}
			<EntityGraphView {elements} focusId={effectiveFocusId} onNavigate={onNavigate} />
			<div class="graph-legend">
				<div class="legend-item">
					<span class="legend-box"></span>
					<span>Caller/Callee</span>
				</div>
			</div>
		{:else}
			<ol class="chain-list">
				{#each nodes as node, i}
					<li>
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="chain-item"
							tabindex="0"
							role="button"
							onclick={() => onNavigate(node.function_id)}
							onkeydown={(e) => handleItemKeydown(e, node.function_id)}
						>
							<span class="chain-number">{i + 1}</span>
							<span class="chain-name">{node.function_name}</span>
							<span class="chain-location">{node.file_path.split('/').pop()}:{node.call_line ?? ''}</span>
						</div>
					</li>
				{/each}
			</ol>
		{/if}
	{/if}
</div>

<style>
	.call-graph-container {
		border: 1px solid var(--gray-200);
		padding: 1rem;
	}

	.empty-state {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}

	.view-toggle {
		display: flex;
		gap: 0;
		margin-bottom: 1rem;
	}

	.toggle-btn {
		padding: 0.4rem 0.85rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		border-right: none;
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	.toggle-btn:last-child {
		border-right: 1px solid var(--gray-300);
	}

	.toggle-btn.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
	}

	.chain-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 0.5rem;
	}

	.chain-item {
		padding: 0.75rem 1rem;
		border: 1px solid var(--black);
		cursor: pointer;
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 1rem;
		align-items: center;
	}

	.chain-item:hover {
		background-color: var(--gray-100);
	}

	.chain-number {
		font-family: 'Space Mono', monospace;
		color: var(--gray-400);
	}

	.chain-name {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 700;
	}

	.chain-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-400);
	}

	.graph-legend {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
		display: flex;
		gap: 2rem;
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.85rem;
	}

	.legend-box {
		width: 20px;
		height: 20px;
		border: 2px solid var(--black);
		background: var(--white);
	}
</style>
