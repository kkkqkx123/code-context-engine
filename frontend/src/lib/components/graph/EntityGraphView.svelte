<script lang="ts">
	/**
	 * Embedded graph view for entity scoped relationships.
	 *
	 * Wraps GraphCanvas with the viewport toolbar so entity tabs (call graph,
	 * inheritance) reuse the same renderer, stylesheet and layout model as the
	 * full graph explorer instead of maintaining bespoke SVG or list only
	 * visualizations. The wrapper owns the Cytoscape Core instance via bind:cy
	 * and drives viewport operations directly, mirroring the explorer page.
	 */
	import type { Core } from 'cytoscape';
	import GraphCanvas, {
		layoutOptions,
		type GraphLayoutName,
	} from '$lib/components/graph/GraphCanvas.svelte';
	import GraphToolbar from '$lib/components/graph/GraphToolbar.svelte';
	import type { GraphElement } from '$lib/utils/graph-elements';
	import {
		fitGraphViewport,
		zoomGraphViewport,
		resetGraphViewport,
		exportGraphPng,
	} from '$lib/utils/graph-viewport';

	interface Props {
		elements?: GraphElement[];
		focusId?: string | null;
		initialLayout?: GraphLayoutName;
		minHeight?: number;
		onNavigate?: (id: string) => void;
	}

	let {
		elements = [],
		focusId = null,
		initialLayout = 'breadthfirst',
		minHeight = 360,
		onNavigate = () => {},
	}: Props = $props();

	let cy = $state<Core | null>(null);
	// svelte-ignore state_referenced_locally
	let layout = $state<GraphLayoutName>(initialLayout);
	let showEdgeLabels = $state(false);

	let nodeCount = $derived(
		elements.filter((element) => !('source' in element.data)).length,
	);
	let edgeCount = $derived(elements.length - nodeCount);

	function relayout() {
		cy?.layout(layoutOptions(layout)).run();
	}
</script>

<div class="entity-graph-view">
	<GraphToolbar
		{nodeCount}
		{edgeCount}
		{layout}
		onZoomIn={() => zoomGraphViewport(cy, 1)}
		onZoomOut={() => zoomGraphViewport(cy, -1)}
		onFit={() => fitGraphViewport(cy)}
		onReset={() => resetGraphViewport(cy)}
		onRelayout={relayout}
		onLayoutChange={(value) => (layout = value)}
		onExportPng={() => exportGraphPng(cy)}
	/>
	<GraphCanvas
		bind:cy
		{elements}
		{layout}
		{focusId}
		{minHeight}
		{showEdgeLabels}
		onNodeSelect={onNavigate}
		onNodeActivate={onNavigate}
	/>
	<p class="canvas-hint">
		Click a node to open the entity · Drag to pan · Ctrl/Cmd + wheel to zoom
	</p>
</div>

<style>
	.entity-graph-view {
		border: 1px solid var(--black);
	}

	.entity-graph-view :global(.graph-toolbar) {
		border: none;
		border-bottom: 1px solid var(--gray-200);
	}

	.canvas-hint {
		margin: 0;
		padding: 0.4rem 0.75rem;
		border-top: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		color: var(--gray-500);
	}
</style>
