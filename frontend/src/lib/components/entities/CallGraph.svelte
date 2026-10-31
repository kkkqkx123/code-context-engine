<script lang="ts">
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import type { CallChainNode } from '$lib/api/search';
	import EntityGraphView from '$lib/components/graph/EntityGraphView.svelte';
	import CallChainList from '$lib/components/entities/CallChainList.svelte';
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
</script>

<div class="call-graph-container">
	{#if nodes.length === 0}
		<EmptyState padded message="No call graph data available" />
	{:else}
		<div class="view-toggle" role="tablist" aria-label="Call graph view mode">
			<Button
				variant="tab"
				active={viewMode === 'graph'}
				onclick={() => (viewMode = 'graph')}
			>
				Graph
			</Button>
			<Button
				variant="tab"
				active={viewMode === 'list'}
				onclick={() => (viewMode = 'list')}
			>
				List
			</Button>
		</div>

		{#if viewMode === 'graph'}
			<EntityGraphView {elements} focusId={effectiveFocusId} {onNavigate} />
			<div class="graph-legend">
				<div class="legend-item">
					<span class="legend-box"></span>
					<span>Caller/Callee</span>
				</div>
			</div>
		{:else}
			<CallChainList {nodes} {onNavigate} />
		{/if}
	{/if}
</div>

<style>
	.call-graph-container {
		border: 1px solid var(--gray-200);
		padding: 1rem;
	}

	.view-toggle {
		display: flex;
		gap: 0;
		margin-bottom: 1rem;
		border: 1px solid var(--gray-300);
		width: fit-content;
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
