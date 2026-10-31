<script lang="ts">
	/**
	 * Graph viewport control strip.
	 *
	 * Mirrors the viewport affordances the reference graph tooling exposes
	 * (zoom in/out, fit, reset) and adds layout switching plus a node count
	 * readout so the user can tell how much of the graph is loaded.
	 */
	import type { GraphLayoutName } from './GraphCanvas.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';

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
		onExportPng?: () => void;
	}

	let {
		nodeCount = 0,
		edgeCount = 0,
		layout = $bindable('cose-bilkent' as GraphLayoutName),
		loading = false,
		onZoomIn = () => {},
		onZoomOut = () => {},
		onFit = () => {},
		onReset = () => {},
		onRelayout = () => {},
		onLayoutChange = () => {},
		onExportPng = () => {},
	}: Props = $props();

	const layouts: Array<{ value: GraphLayoutName; label: string }> = [
		{ value: 'cose-bilkent', label: 'Force' },
		{ value: 'cose', label: 'Force (basic)' },
		{ value: 'breadthfirst', label: 'Hierarchy' },
		{ value: 'concentric', label: 'Concentric' },
		{ value: 'grid', label: 'Grid' },
	];

	function handleLayoutChange(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement)
			.value as GraphLayoutName;
		layout = value;
		onLayoutChange(value);
	}
</script>

<div class="graph-toolbar">
	<div class="toolbar-group">
		<Button
			variant="ghost"
			size="xs"
			onclick={onZoomOut}
			aria-label="Zoom out"
			title="Zoom out"
		>
			<Icon name="minus" />
		</Button>
		<Button
			variant="ghost"
			size="xs"
			onclick={onZoomIn}
			aria-label="Zoom in"
			title="Zoom in"
		>
			+
		</Button>
		<Button
			variant="ghost"
			size="xs"
			onclick={onFit}
			title="Fit graph to viewport"
		>
			Fit
		</Button>
		<Button variant="ghost" size="xs" onclick={onReset} title="Reset viewport">
			Reset
		</Button>
	</div>

	<div class="toolbar-group">
		<label class="tool-label" for="graph-layout">Layout</label>
		<select
			id="graph-layout"
			class="tool-select"
			value={layout}
			onchange={handleLayoutChange}
		>
			{#each layouts as item (item.value)}
				<option value={item.value}>{item.label}</option>
			{/each}
		</select>
		<Button
			variant="ghost"
			size="xs"
			onclick={onRelayout}
			title="Re-run the current layout"
		>
			Re-layout
		</Button>
		<Button
			variant="ghost"
			size="xs"
			onclick={onExportPng}
			title="Export the current view as PNG"
		>
			Export PNG
		</Button>
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
