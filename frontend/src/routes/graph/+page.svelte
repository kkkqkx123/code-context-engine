<script lang="ts">
	/**
	 * Graph explorer page.
	 *
	 * Composes the canvas, viewport toolbar, filter panel and node inspector
	 * over a single shared graph store. The page owns the seed strategy (an
	 * entity oriented load or a bounded project overview) while the store owns
	 * accumulation and cache invalidation.
	 */
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import GraphCanvas, { type GraphLayoutName } from '$lib/components/graph/GraphCanvas.svelte';
	import GraphToolbar from '$lib/components/graph/GraphToolbar.svelte';
	import GraphFilterPanel from '$lib/components/graph/GraphFilterPanel.svelte';
	import { graphState, graphActions, activeDomains } from '$lib/stores/graph';
	import { currentProjectId } from '$lib/stores/project';
	import {
		CONFIDENCE_META,
		RELATION_DOMAINS,
		edgeConfidence,
		relationDomain,
		relationLabel
	} from '$lib/utils/graph-style';

	type SeedMode = 'focus' | 'overview';

	let canvas: GraphCanvas | null = $state(null);
	let layout: GraphLayoutName = $state('cose');
	let seedMode: SeedMode = $state('focus');
	let seedId = $state('');
	let selectedId: string | null = $state(null);
	let impactFile = $state('');

	// The query string may carry an entity to focus on, which is how the entity
	// detail page hands off to the explorer.
	let queryEntity = $derived(page.url.searchParams.get('entity') ?? '');
	let queryFile = $derived(page.url.searchParams.get('file') ?? '');

	let store = $derived($graphState);
	let nodes = $derived(store.nodes);
	let edges = $derived(store.edges);
	let availableDomains = $derived([...activeDomains(edges)]);
	let visibleEdges = $derived(
		store.filters.hideAmbiguous
			? edges.filter((edge) => edgeConfidence(edge.confidence) !== 'ambiguous')
			: edges
	);

	let selectedNode = $derived(nodes.find((node) => node.id === selectedId) ?? null);
	let selectedEdges = $derived(
		selectedId === null
			? []
			: edges.filter((edge) => edge.source === selectedId || edge.target === selectedId)
	);

	let projectId = $derived($currentProjectId);

	onMount(async () => {
		if (queryEntity) {
			seedMode = 'focus';
			seedId = queryEntity;
			await graphActions.loadEgo(queryEntity, 2, 'both');
			selectedId = queryEntity;
		} else {
			await graphActions.loadOverview(400);
		}
		if (queryFile) {
			impactFile = queryFile;
			await graphActions.loadImpact(queryFile);
		}
		await graphActions.loadComponents();
	});

	async function runSeed() {
		const id = seedId.trim();
		if (seedMode === 'focus') {
			if (!id) return;
			selectedId = id;
			await graphActions.loadEgo(id, 2, 'both');
		} else {
			await graphActions.loadOverview(400);
		}
		await graphActions.loadComponents();
	}

	function handleSeedKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			runSeed();
		}
	}

	/** Double click on a node pulls in its immediate neighborhood. */
	async function handleNodeActivate(nodeId: string) {
		await graphActions.expand(nodeId, 1, 'both');
		canvas?.relayout();
	}

	function handleNodeSelect(nodeId: string) {
		selectedId = nodeId;
	}

	function openEntity(nodeId: string) {
		window.location.href = `/entities/${encodeURIComponent(nodeId)}`;
	}

	async function runImpact() {
		const file = impactFile.trim();
		if (!file) {
			graphActions.clearImpact();
			return;
		}
		await graphActions.loadImpact(file);
	}

	function clearImpact() {
		impactFile = '';
		graphActions.clearImpact();
	}

	function toggleAmbiguous() {
		graphActions.setFilters({ hideAmbiguous: !store.filters.hideAmbiguous });
	}

	function domainLabel(relation: string) {
		return RELATION_DOMAINS[relationDomain(relation)].label;
	}

	$effect(() => {
		// Reset accumulated graph state when the selected project changes.
		void projectId;
	});
</script>

<svelte:head>
	<title>Graph Explorer - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Graph Explorer"
			subtitle="Explore calls, dependencies, structure and references as one graph"
		/>

		<div class="seed-bar">
			<div class="seed-mode">
				<button
					type="button"
					class="mode-btn"
					class:active={seedMode === 'focus'}
					onclick={() => (seedMode = 'focus')}
				>
					Focus entity
				</button>
				<button
					type="button"
					class="mode-btn"
					class:active={seedMode === 'overview'}
					onclick={() => (seedMode = 'overview')}
				>
					Project overview
				</button>
			</div>

			{#if seedMode === 'focus'}
				<input
					class="seed-input"
					type="text"
					placeholder="Entity id…"
					bind:value={seedId}
					onkeydown={handleSeedKeydown}
					aria-label="Entity id to focus on"
				/>
			{/if}

			<button type="button" class="primary-btn" onclick={runSeed} disabled={store.loading}>
				{store.loading ? 'Loading…' : 'Load'}
			</button>

			<div class="seed-spacer"></div>

			<input
				class="seed-input file-input"
				type="text"
				placeholder="Impact: file path…"
				bind:value={impactFile}
				onkeydown={(e) => e.key === 'Enter' && runImpact()}
				aria-label="File path for impact analysis"
			/>
			<button type="button" class="ghost-btn" onclick={runImpact}>Impact</button>
			{#if store.meta.impactFile}
				<button type="button" class="ghost-btn" onclick={clearImpact}>Clear</button>
			{/if}
		</div>

		{#if store.error}
			<div class="error-banner">
				<span>{store.error}</span>
			</div>
		{/if}

		{#if store.truncated}
			<div class="warn-banner">
				Node limit reached. Expand is disabled until you reload a smaller seed.
			</div>
		{/if}

		<div class="workspace">
			<GraphFilterPanel
				domains={store.filters.domains}
				{availableDomains}
				search={store.filters.search}
				hideAmbiguous={store.filters.hideAmbiguous}
				onToggleDomain={(domain) => {
					graphActions.toggleDomain(domain);
					canvas?.relayout();
				}}
				onSearch={(value) => graphActions.setSearch(value)}
				onToggleAmbiguous={toggleAmbiguous}
			/>

			<div class="canvas-column">
				<GraphToolbar
					nodeCount={nodes.length}
					edgeCount={visibleEdges.length}
					{layout}
					loading={store.loading}
					onZoomIn={() => canvas?.zoomBy(0.2)}
					onZoomOut={() => canvas?.zoomBy(-0.2)}
					onFit={() => canvas?.fit()}
					onReset={() => canvas?.resetView()}
					onRelayout={() => canvas?.relayout()}
					onLayoutChange={(value) => (layout = value)}
				/>
				<GraphCanvas
					bind:this={canvas}
					elements={$graphState.elements}
					{layout}
					focusId={store.meta.focusId}
					visibleDomains={store.filters.domains}
					search={store.filters.search}
					impactDirect={store.meta.impactDirect}
					impactTransitive={store.meta.impactTransitive}
					onNodeSelect={handleNodeSelect}
					onNodeActivate={handleNodeActivate}
				/>
				<p class="canvas-hint">
					Drag to pan · Ctrl/Cmd + wheel to zoom · Double click a node to expand its neighborhood
				</p>
			</div>

			<aside class="inspector" aria-label="Node inspector">
				{#if selectedNode}
					<h4 class="inspector-title">{selectedNode.label}</h4>
					<dl class="meta-list">
						<dt>Kind</dt>
						<dd>{selectedNode.kind}</dd>
						<dt>Location</dt>
						<dd class="mono">{selectedNode.source_file}:{selectedNode.source_location}</dd>
						<dt>Community</dt>
						<dd>{store.meta.communities[selectedNode.id] ?? '—'}</dd>
					</dl>

					<div class="inspector-actions">
						<button type="button" class="ghost-btn" onclick={() => openEntity(selectedNode.id)}>
							Open entity
						</button>
						<button
							type="button"
							class="ghost-btn"
							onclick={() => handleNodeActivate(selectedNode.id)}
						>
							Expand
						</button>
						<button
							type="button"
							class="ghost-btn"
							onclick={() => canvas?.centerOn(selectedNode.id)}
						>
							Center
						</button>
					</div>

					<h5 class="subsection-title">Relations ({selectedEdges.length})</h5>
					<ul class="relation-list">
						{#each selectedEdges.slice(0, 40) as edge (edge.source + edge.target + edge.relation)}
							{@const outgoing = edge.source === selectedNode.id}
							{@const confidence = edgeConfidence(edge.confidence)}
							<li class="relation-item">
								<div class="relation-head">
									<Badge variant={outgoing ? 'active' : 'default'}>
										{outgoing ? 'OUT' : 'IN'}
									</Badge>
									<span class="relation-domain">{domainLabel(edge.relation)}</span>
								</div>
								<p class="relation-name">{relationLabel(edge.relation)}</p>
								<p class="relation-target mono">
									{outgoing ? edge.target : edge.source}
								</p>
								<p class="relation-confidence" title={CONFIDENCE_META[confidence].description}>
									{CONFIDENCE_META[confidence].label}
								</p>
							</li>
						{/each}
						{#if selectedEdges.length === 0}
							<li class="relation-empty">No loaded relations for this node.</li>
						{/if}
					</ul>
				{:else}
					<div class="inspector-empty">
						<p>No node selected</p>
						<span>Click a node in the canvas to inspect it</span>
					</div>
				{/if}
			</aside>
		</div>

		<div class="legend">
			{#each Object.values(RELATION_DOMAINS) as domain (domain.domain)}
				<div class="legend-item">
					<span class="legend-line" style="--line: {domain.color}"></span>
					<span class="legend-label">{domain.label}</span>
				</div>
			{/each}
			<div class="legend-item">
				<span class="legend-node"></span>
				<span class="legend-label">Entity</span>
			</div>
		</div>
	</div>
</div>

<style>
	.seed-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.75rem;
		border: 1px solid var(--black);
		margin-bottom: 1rem;
	}

	.seed-mode {
		display: flex;
	}

	.mode-btn {
		padding: 0.4rem 0.75rem;
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

	.mode-btn:last-child {
		border-right: 1px solid var(--gray-300);
	}

	.mode-btn.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
	}

	.seed-input {
		height: 30px;
		min-width: 220px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.seed-input:focus {
		outline: none;
		border-color: var(--black);
	}

	.file-input {
		min-width: 200px;
	}

	.seed-spacer {
		flex: 1;
	}

	.primary-btn,
	.ghost-btn {
		height: 30px;
		padding: 0 0.85rem;
		border: 1px solid var(--black);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: all 0.15s;
	}

	.primary-btn {
		background: var(--black);
		color: var(--white);
	}

	.primary-btn:disabled {
		opacity: 0.5;
		cursor: wait;
	}

	.ghost-btn {
		background: var(--white);
		color: var(--black);
	}

	.ghost-btn:hover {
		background: var(--gray-100);
	}

	.error-banner,
	.warn-banner {
		padding: 0.6rem 0.75rem;
		margin-bottom: 1rem;
		border: 1px solid;
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
	}

	.error-banner {
		border-color: var(--danger);
		background: var(--danger-bg);
		color: var(--danger);
	}

	.warn-banner {
		border-color: var(--warning);
		background: var(--warning-bg);
		color: var(--warning);
	}

	.workspace {
		display: grid;
		grid-template-columns: 220px minmax(0, 1fr) 280px;
		gap: 1rem;
		align-items: start;
	}

	.canvas-column {
		min-width: 0;
		border: 1px solid var(--black);
	}

	.canvas-column :global(.graph-canvas) {
		border-bottom: none;
	}

	.canvas-hint {
		margin: 0;
		padding: 0.4rem 0.75rem;
		border-top: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		color: var(--gray-500);
	}

	.inspector {
		border: 1px solid var(--black);
		padding: 1rem;
		min-height: 200px;
	}

	.inspector-title {
		font-size: 1.05rem;
		margin: 0 0 0.75rem;
		word-break: break-word;
	}

	.meta-list {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.25rem 0.75rem;
		margin: 0 0 1rem;
	}

	.meta-list dt {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.meta-list dd {
		margin: 0;
		font-size: 0.78rem;
		color: var(--black);
		word-break: break-all;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.inspector-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--gray-200);
		margin-bottom: 0.75rem;
	}

	.subsection-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		font-weight: 400;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		margin: 0 0 0.5rem;
	}

	.relation-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		max-height: 420px;
		overflow-y: auto;
	}

	.relation-item {
		border-left: 2px solid var(--gray-200);
		padding-left: 0.5rem;
	}

	.relation-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.relation-domain {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.relation-name {
		margin: 0.15rem 0 0;
		font-size: 0.78rem;
		color: var(--black);
	}

	.relation-target {
		margin: 0;
		color: var(--gray-500);
		word-break: break-all;
	}

	.relation-confidence {
		margin: 0.15rem 0 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-400);
	}

	.relation-empty,
	.inspector-empty {
		font-size: 0.78rem;
		color: var(--gray-400);
		font-style: italic;
	}

	.inspector-empty {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 2rem 0;
		text-align: center;
	}

	.inspector-empty p {
		margin: 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		font-style: normal;
	}

	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.legend-line {
		width: 22px;
		border-top: 2px solid var(--line);
	}

	.legend-node {
		width: 12px;
		height: 12px;
		border: 1.5px solid var(--black);
		background: var(--white);
	}

	.legend-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	@media (max-width: 1200px) {
		.workspace {
			grid-template-columns: 1fr;
		}

		.inspector {
			min-height: auto;
		}
	}
</style>
