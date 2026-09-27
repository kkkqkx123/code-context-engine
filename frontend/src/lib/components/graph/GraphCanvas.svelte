<script lang="ts">
	/**
	 * Cytoscape canvas wrapper.
	 *
	 * Owns the renderer instance, keeps it in sync with the element array, and
	 * forwards interaction intents to the parent. Layout selection and element
	 * styling come from the shared presentation model so the canvas, legend and
	 * filters never disagree.
	 */
	import { onMount, onDestroy } from 'svelte';
	import type { Core, ElementDefinition, LayoutOptions, StylesheetCSS } from 'cytoscape';
	import {
		graphStylesheet,
		type GraphElement,
		type RelationDomain
	} from '$lib/utils/graph-style';

	export type GraphLayoutName =
		| 'cose-bilkent'
		| 'cose'
		| 'breadthfirst'
		| 'concentric'
		| 'grid';

	interface Props {
		elements?: GraphElement[];
		/** Layout used for the initial render and when the layout changes. */
		layout?: GraphLayoutName;
		/** Node id to highlight; typically the entity the graph is centered on. */
		focusId?: string | null;
		/** Domains to keep visible; edges outside the list are hidden. */
		visibleDomains?: RelationDomain[];
		/** Node ids flagged as direct impact of a changed file. */
		impactDirect?: string[];
		/** Node ids flagged as transitive impact of a changed file. */
		impactTransitive?: string[];
		/** Case-insensitive substring used to highlight matching nodes. */
		search?: string;
		/** Whether to render the relation label on each edge. */
		showEdgeLabels?: boolean;
		onNodeSelect?: (nodeId: string) => void;
		/** Fired on double click, used to expand the neighborhood of a node. */
		onNodeActivate?: (nodeId: string) => void;
		cy?: Core | null;
		containerElement?: HTMLDivElement | null;
	}

	let {
		elements = [],
		layout = 'cose-bilkent',
		focusId = null,
		visibleDomains = ['call', 'dependency', 'structural', 'reference', 'other'],
		impactDirect = [],
		impactTransitive = [],
		search = '',
		showEdgeLabels = false,
		onNodeSelect = () => {},
		onNodeActivate = () => {},
		cy = $bindable(null),
		containerElement = $bindable(null)
	}: Props = $props();

	let container: HTMLDivElement | null = null;
	let lastElementCount = $state(0);
	let bilkentRegistered = false;

	function layoutOptions(name: GraphLayoutName): LayoutOptions {
		switch (name) {
			case 'breadthfirst':
				return {
					name: 'breadthfirst',
					directed: true,
					spacingFactor: 1.1,
					animate: false
				} as unknown as LayoutOptions;
			case 'concentric':
				// Rank by degree so hubs land in the center ring.
				return {
					name: 'concentric',
					minNodeSpacing: 24,
					animate: false
				} as unknown as LayoutOptions;
			case 'grid':
				return { name: 'grid', avoidOverlap: true, animate: false } as unknown as LayoutOptions;
			case 'cose-bilkent':
				if (!bilkentRegistered) return layoutOptions('cose');
				return {
					name: 'cose-bilkent',
					animate: 'end',
					animationDuration: 400,
					randomize: true,
					idealEdgeLength: 100,
					nodeRepulsion: 4500,
					nodeSeparation: 75,
					edgeElasticity: 0.45,
					nestingFactor: 0.1,
					gravity: 0.25,
					numIter: 2250,
					tile: true,
					tilingPaddingVertical: 10,
					tilingPaddingHorizontal: 10,
					packComponents: true
				} as unknown as LayoutOptions;
			case 'cose':
			default:
				return {
					name: 'cose',
					animate: false,
					nodeRepulsion: 4000,
					idealEdgeLength: 90,
					nodeOverlap: 12,
					numIter: 700
				} as unknown as LayoutOptions;
		}
	}

	/** Apply epoch-independent visual state derived from props. */
	function applyDecorations() {
		if (!cy) return;

		cy.batch(() => {
			cy!.elements().removeClass('focus dimmed impact impact-transitive highlighted');

			const direct = new Set(impactDirect);
			const transitive = new Set(impactTransitive);

			cy!.nodes().forEach((node) => {
				const id = node.id();
				if (direct.has(id)) node.addClass('impact');
				else if (transitive.has(id)) node.addClass('impact-transitive');

				if (focusId && id === focusId) node.addClass('focus');

				if (search && search.trim().length >= 2) {
					const needle = search.trim().toLowerCase();
					const label = String(node.data('label') ?? '').toLowerCase();
					if (!label.includes(needle)) node.addClass('dimmed');
				}
			});
		});
	}

	/** Show or hide edges by relation domain. */
	function applyDomainFilter() {
		if (!cy) return;
		const allowed = new Set(visibleDomains);
		cy.batch(() => {
			cy!.edges().forEach((edge) => {
				const domain = edge.data('domain') as RelationDomain;
				edge.style('display', allowed.has(domain) ? 'element' : 'none');
			});
		});
	}

	function createInstance(cytoscape: typeof import('cytoscape')) {
		if (!container) return;
		const instance = cytoscape({
			container,
			elements: elements as unknown as ElementDefinition[],
			style: graphStylesheet as unknown as StylesheetCSS[],
			layout: layoutOptions(layout),
			wheelSensitivity: 0.25,
			boxSelectionEnabled: false,
			selectionType: 'single'
		});
		cy = instance;

		containerElement = container;
		lastElementCount = elements.length;

		instance.on('tap', 'node', (event) => {
			onNodeSelect(event.target.id());
		});
		instance.on('dbltap', 'node', (event) => {
			onNodeActivate(event.target.id());
		});

		applyDomainFilter();
		applyDecorations();
	}

	onMount(() => {
		if (!container) return;

		let mounted = true;
		// The renderer and the layout extension touch the DOM at import time, so
		// they are loaded lazily and only inside the browser lifecycle to keep
		// server rendering untouched.
		Promise.all([import('cytoscape'), import('cytoscape-cose-bilkent')])
			.then(([cyModule, bilkentModule]) => {
				if (!mounted || !container) return;
				const cytoscape = cyModule.default;
				try {
					cytoscape.use(bilkentModule.default);
					bilkentRegistered = true;
				} catch {
					bilkentRegistered = false;
				}
				createInstance(cytoscape);
			})
			.catch(() => {
				// If the layout extension fails to load, fall back to plain cytoscape.
				if (!mounted || !container) return;
				import('cytoscape').then((module) => {
					if (!mounted || !container) return;
					createInstance(module.default);
				});
			});

		return () => {
			mounted = false;
			cy?.destroy();
			cy = null;
		};
	});

	// Live sync: add and remove elements without rebuilding the instance.
	$effect(() => {
		if (!cy) return;
		const incoming = elements;
		const incomingIds = new Set(incoming.map((element) => element.data.id));
		let mutated = false;

		cy.batch(() => {
			cy!.elements().forEach((element) => {
				if (!incomingIds.has(element.id())) {
					element.remove();
					mutated = true;
				}
			});

			const existing = new Set(cy!.elements().map((element) => element.id()));
			const additions = incoming.filter((element) => !existing.has(element.data.id));
			if (additions.length > 0) {
				cy!.add(additions as unknown as ElementDefinition[]);
				mutated = true;
			}
		});

		if (mutated) {
			cy.layout(layoutOptions(layout)).run();
		}
		lastElementCount = incoming.length;
		applyDomainFilter();
		applyDecorations();
	});

	// Re-run layout when the layout strategy changes.
	$effect(() => {
		const name = layout;
		if (!cy) return;
		cy.layout(layoutOptions(name)).run();
	});

	$effect(() => {
		// Track filter inputs so decoration stays current.
		void visibleDomains;
		void focusId;
		void search;
		void impactDirect;
		void impactTransitive;
		if (cy) {
			applyDomainFilter();
			applyDecorations();
		}
	});

	// Toggle edge relation labels without rebuilding the instance.
	$effect(() => {
		if (!cy) return;
		void showEdgeLabels;
		cy.edges().style('label', showEdgeLabels ? 'data(relationLabel)' : '');
	});

	export function fit() {
		cy?.fit(undefined, 40);
	}

	export function zoomBy(delta: number) {
		if (!cy) return;
		cy.zoom({ level: Math.min(3, Math.max(0.15, cy.zoom() + delta)), renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
	}

	export function resetView() {
		if (!cy) return;
		cy.zoom(1);
		cy.center();
	}

	export function centerOn(nodeId: string) {
		if (!cy) return;
		const node = cy.getElementById(nodeId);
		if (node.length === 0) return;
		cy.animate({ center: { eles: node }, zoom: Math.max(cy.zoom(), 1) }, { duration: 250 });
	}

	export function relayout() {
		cy?.layout(layoutOptions(layout)).run();
	}

	export function exportPng(): string {
		if (!cy) return '';
		return cy.png({ full: true, scale: 2, bg: '#ffffff' });
	}
</script>

<div class="graph-canvas" bind:this={container} role="application" aria-label="Relation graph canvas">
	{#if elements.length === 0}
		<div class="canvas-empty">
			<p>No graph data</p>
			<span>Select an entity to explore its relationships</span>
		</div>
	{/if}
</div>

<style>
	.graph-canvas {
		position: relative;
		width: 100%;
		height: 100%;
		min-height: 520px;
		background: var(--white);
		background-image:
			linear-gradient(var(--gray-100) 1px, transparent 1px),
			linear-gradient(90deg, var(--gray-100) 1px, transparent 1px);
		background-size: 24px 24px;
	}

	.canvas-empty {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		pointer-events: none;
	}

	.canvas-empty p {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		margin: 0;
	}

	.canvas-empty span {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 0.85rem;
		color: var(--gray-400);
	}
</style>