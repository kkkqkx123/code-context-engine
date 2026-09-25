<script lang="ts">
	/**
	 * Graph viewport control strip.
	 *
	 * Mirrors the viewport affordances the reference graph tooling exposes
	 * (zoom in/out, fit, reset) and adds layout switching plus a node count
	 * readout so the user can tell how much of the graph is loaded.
	 */
	import type { GraphLayoutName } from './GraphCanvas.svelte';

	interface Props {
		nodeCount?: number;
		edgeCount?: number;
		layout?: GraphLayoutName;
		loading?: boolean;
		onZoomIn?: () => void;
		onZoomOut?: () => void;
		onFit?: () => void;
		onReset?: () => void;
		onRelayout?: () => void;
		onLayoutChange?: (layout: GraphLayoutName) => void;
	}

	let {
		nodeCount = 0,
		edgeCount = 0,
		layout = $bindable('cose' as GraphLayoutName),
		loading = false,
		onZoomIn = () => {},
		onZoomOut = () => {},
		onFit = () => {},
		onReset = () => {},
		onRelayout = () => {},
		onLayoutChange = () => {}
	}: Props = $props();

	const layouts: Array<{ value: GraphLayoutName; label: string }> = [
		{ value: 'cose', label: 'Force' },
		{ value: 'breadthfirst', label: 'Hierarchy' },
		{ value: 'concentric', label: 'Concentric' },
		{ value: 'grid', label: 'Grid' }
	];

	function handleLayoutChange(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement).value as GraphLayoutName;
		layout = value;
		onLayoutChange(value);
	}
</script>

<div class="graph-toolbar">
	<div class="toolbar-group">
		<button type="button" class="tool-btn" onclick={onZoomOut} aria-label="Zoom out" title="Zoom out">−</button>
		<button type="button" class="tool-btn" onclick={onZoomIn} aria-label="Zoom in" title="Zoom in">+</button>
		<button type="button" class="tool-btn wide" onclick={onFit} title="Fit graph to viewport">Fit</button>
		<button type="button" class="tool-btn wide" onclick={onReset} title="Reset viewport">Reset</button>
	</div>

	<div class="toolbar-group">
		<label class="tool-label" for="graph-layout">Layout</label>
		<select id="graph-layout" class="tool-select" value={layout} onchange={handleLayoutChange}>
			{#each layouts as item (item.value)}
				<option value={item.value}>{item.label}</option>
			{/each}
		</select>
		<button type="button" class="tool-btn wide" onclick={onRelayout} title="Re-run the current layout">
			Re-layout
		</button>
	</div>

	<div class="toolbar-spacer"></div>

	<div class="toolbar-stats">
		{#if loading}
			<span class="stat loading">Loading…</span>
		{:else}
			<span class="stat"><strong>{nodeCount}</strong> nodes</span>
			<span class="stat"><strong>{edgeCount}</strong> edges</span>
		{/if}
	</div>
</div>

<style>
	.graph-toolbar {
		display: flex;
		align-items: center;
		gap: 1rem;
		flex-wrap: wrap;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--black);
		border-bottom: none;
		background: var(--white);
	}

	.toolbar-group {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.tool-btn {
		height: 28px;
		min-width: 30px;
		padding: 0 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--black);
		transition: all 0.15s;
	}

	.tool-btn.wide {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.tool-btn:hover {
		border-color: var(--black);
		background: var(--gray-100);
	}

	.tool-btn:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.tool-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.tool-select {
		height: 28px;
		padding: 0 0.4rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--black);
	}

	.tool-select:focus {
		outline: none;
		border-color: var(--black);
	}

	.toolbar-stats {
		display: flex;
		gap: 0.75rem;
	}

	.stat {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-500);
	}

	.stat strong {
		color: var(--black);
	}
</style>
