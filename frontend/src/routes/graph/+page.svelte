<script lang="ts">
	/**
	 * Graph explorer page.
	 *
	 * Composes the canvas, viewport toolbar, filter panel and node inspector
	 * over a single shared graph store. The page owns the seed strategy (an
	 * entity oriented load, an explicit subgraph, a path search or a bounded
	 * project overview) while the store owns accumulation and cache
	 * invalidation.
	 *
	 * The Cytoscape Core instance is owned directly by this page via bind:cy.
	 * All imperative viewport operations (zoom/fit/relayout/export) call
	 * Cytoscape methods directly on `cy` rather than going through wrapper
	 * functions on the canvas component.
	 */
	import { onDestroy, onMount } from 'svelte';
	import type { Core } from 'cytoscape';
	import { page } from '$app/state';
	import { SvelteMap } from 'svelte/reactivity';
	import { watchState, watchActions } from '$lib/stores/watch';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import GraphCanvas, {
		layoutOptions,
		type GraphLayoutName,
	} from '$lib/components/graph/GraphCanvas.svelte';
	import GraphToolbar from '$lib/components/graph/GraphToolbar.svelte';
	import GraphFilterPanel from '$lib/components/graph/GraphFilterPanel.svelte';
	import { graphState, graphActions, activeDomains } from '$lib/stores/graph';
	import { onProjectChange } from '$lib/stores/project';
	import type { GraphDirection, GraphEdge } from '$lib/api/graph';
	import type { SymbolCandidate } from '$lib/api/client';
	import {
		CONFIDENCE_META,
		NODE_KINDS,
		NODE_SHAPE_LEGEND,
		RELATION_DOMAINS,
		edgeConfidence,
		edgeDomain,
		normalizeNodeKind,
		relationLabel,
		relationLineStyle,
	} from '$lib/utils/graph-style';

	type SeedMode = 'focus' | 'overview' | 'path' | 'subgraph';

	let cy = $state<Core | null>(null);
	let layout: GraphLayoutName = $state('cose-bilkent');
	let seedMode: SeedMode = $state('focus');
	let seedId = $state('');
	let selectedId: string | null = $state(null);
	let impactFile = $state('');
	let pathStart = $state('');
	let pathEnd = $state('');
	let subgraphIds = $state('');
	let egoDepth = $state(2);
	let egoDirection = $state<GraphDirection>('both');
	let showEdgeLabels = $state(false);

	// The query string may carry an entity to focus on, which is how the entity
	// detail page hands off to the explorer.
	let queryEntity = $derived(page.url.searchParams.get('entity') ?? '');
	let queryFile = $derived(page.url.searchParams.get('file') ?? '');

	let store = $derived($graphState);
	let nodes = $derived(store.nodes);
	let edges = $derived(store.edges);
	let availableDomains = $derived([...activeDomains(edges)]);

	let selectedNode = $derived(
		nodes.find((node) => node.id === selectedId) ?? null,
	);
	let selectedEdges = $derived(
		selectedId === null
			? []
			: edges.filter(
					(edge) => edge.source === selectedId || edge.target === selectedId,
				),
	);

	/** Seed the canvas from the URL the way a fresh visit does. */
	async function loadInitialGraph() {
		if (queryEntity) {
			seedMode = 'focus';
			seedId = queryEntity;
			await graphActions.loadEgo(queryEntity, egoDepth, egoDirection);
			selectedId = queryEntity;
		} else {
			// Nothing seeded from the URL: show the whole project graph and keep
			// the seed mode in sync so a project switch re-loads the same view.
			seedMode = 'overview';
			await graphActions.loadOverview(400);
		}
		if (queryFile) {
			impactFile = queryFile;
			await graphActions.loadImpact(queryFile);
		}
		await graphActions.loadComponentsIfStale();
	}

	onMount(async () => {
		await loadInitialGraph();
		await watchActions.loadStatus();
		watchActions.startVersionPoll();
	});

	onDestroy(() => {
		watchActions.stopVersionPoll();
	});

	// The canvas only ever holds one project's nodes: clear the previous working
	// set and re-run the current seed against the newly selected project.
	$effect(() =>
		onProjectChange((projectId) => {
			graphActions.reset(projectId);
			void runSeed();
		}),
	);

	async function runSeed() {
		const id = seedId.trim();
		if (seedMode === 'focus') {
			if (!id) return;
			selectedId = id;
			await graphActions.loadEgo(id, egoDepth, egoDirection);
		} else if (seedMode === 'path') {
			const start = pathStart.trim();
			const end = pathEnd.trim();
			if (!start || !end) return;
			await graphActions.loadPath(start, end);
		} else if (seedMode === 'subgraph') {
			const ids = subgraphIds
				.split(',')
				.map((s) => s.trim())
				.filter(Boolean);
			if (ids.length === 0) return;
			await graphActions.loadSubgraph(ids);
		} else {
			await graphActions.loadOverview(400);
		}
		await graphActions.loadComponentsIfStale();
	}

	function handleSeedKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			runSeed();
		}
	}

	// Disambiguation candidates shown when a symbol-name seed matches multiple
	// entities (backend AMBIGUOUS_SYMBOL). Picking one retries with its stable id.
	let candidates: SymbolCandidate[] = $derived(store.error?.candidates ?? []);

	// The backend advanced past the epoch this working set was built from.
	// Banner only: reload stays explicit so in-progress expansion is never lost.
	let watchEpoch = $derived($watchState.status?.relation_epoch ?? null);
	let isStale = $derived(
		watchEpoch !== null &&
			store.meta.epoch !== 0 &&
			watchEpoch > store.meta.epoch,
	);

	function useCandidate(candidate: SymbolCandidate) {
		// Replace whichever ambiguous seed the candidate resolves, then re-run.
		const ambiguousNames = new Set(candidates.map((c) => c.scoped_name));
		const swap = (seed: string) =>
			seed.trim() === candidate.scoped_name || ambiguousNames.has(seed.trim())
				? candidate.stable_id
				: seed;
		if (seedMode === 'focus') {
			seedId = swap(seedId);
		} else if (seedMode === 'path') {
			pathStart = swap(pathStart);
			pathEnd = swap(pathEnd);
		} else if (seedMode === 'subgraph') {
			subgraphIds = subgraphIds.split(',').map(swap).join(',');
		}
		runSeed();
	}

	/** Double click on a node pulls in its immediate neighborhood. */
	async function handleNodeActivate(nodeId: string) {
		await graphActions.expand(nodeId, 1, egoDirection);
		cy?.layout(layoutOptions(layout)).run();
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

	function domainLabel(edge: GraphEdge) {
		return RELATION_DOMAINS[edgeDomain(edge)].label;
	}

	let communities = $derived.by(() => {
		const map = store.meta.communities;
		const groups = new SvelteMap<number, string[]>();
		for (const node of nodes) {
			const c = map[node.id];
			if (c === undefined) continue;
			const arr = groups.get(c) ?? [];
			arr.push(node.id);
			groups.set(c, arr);
		}
		return [...groups.entries()].sort((a, b) => b[1].length - a[1].length);
	});

	async function loadCommunity(ids: string[]) {
		await graphActions.loadSubgraph(ids);
		cy?.layout(layoutOptions(layout)).run();
	}

	function exportPng() {
		if (!cy) return;
		const png = cy.png({ full: true, scale: 2, bg: '#ffffff' });
		const link = document.createElement('a');
		link.href = png;
		link.download = 'graph.png';
		link.click();
	}

	function fitViewport() {
		cy?.fit(undefined, 40);
	}

	function zoomViewport(delta: number) {
		if (!cy) return;
		cy.zoom({
			level: Math.min(3, Math.max(0.15, cy.zoom() + delta)),
			renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 },
		});
	}

	function resetViewport() {
		if (!cy) return;
		cy.zoom(1);
		cy.center();
	}

	function relayout() {
		cy?.layout(layoutOptions(layout)).run();
	}

	function centerOn(nodeId: string) {
		if (!cy) return;
		const node = cy.getElementById(nodeId);
		if (node.length === 0) return;
		cy.animate(
			{ center: { eles: node }, zoom: Math.max(cy.zoom(), 1) },
			{ duration: 250 },
		);
	}
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
				<button
					type="button"
					class="mode-btn"
					class:active={seedMode === 'path'}
					onclick={() => (seedMode = 'path')}
				>
					Find path
				</button>
				<button
					type="button"
					class="mode-btn"
					class:active={seedMode === 'subgraph'}
					onclick={() => (seedMode = 'subgraph')}
				>
					Subgraph
				</button>
			</div>

			{#if seedMode === 'focus'}
				<input
					class="seed-input"
					type="text"
					placeholder="Symbol name, path#name or id…"
					bind:value={seedId}
					onkeydown={handleSeedKeydown}
					aria-label="Entity id to focus on"
				/>
				<label class="inline-field">
					<span class="inline-label">Depth</span>
					<input
						class="seed-input narrow"
						type="number"
						min="1"
						max="4"
						bind:value={egoDepth}
						aria-label="Neighborhood depth"
					/>
				</label>
				<label class="inline-field">
					<span class="inline-label">Dir</span>
					<select
						class="seed-input narrow"
						bind:value={egoDirection}
						aria-label="Traversal direction"
					>
						<option value="both">both</option>
						<option value="out">out</option>
						<option value="in">in</option>
					</select>
				</label>
			{:else if seedMode === 'path'}
				<input
					class="seed-input"
					type="text"
					placeholder="Start entity id…"
					bind:value={pathStart}
					onkeydown={handleSeedKeydown}
					aria-label="Path start entity id"
				/>
				<span class="path-arrow" aria-hidden="true">→</span>
				<input
					class="seed-input"
					type="text"
					placeholder="End entity id…"
					bind:value={pathEnd}
					onkeydown={handleSeedKeydown}
					aria-label="Path end entity id"
				/>
			{:else if seedMode === 'subgraph'}
				<input
					class="seed-input wide-input"
					type="text"
					placeholder="Comma-separated entity ids…"
					bind:value={subgraphIds}
					onkeydown={handleSeedKeydown}
					aria-label="Entity ids for induced subgraph"
				/>
			{/if}

			<button
				type="button"
				class="primary-btn"
				onclick={runSeed}
				disabled={store.loading}
			>
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
			<button type="button" class="ghost-btn" onclick={runImpact}>Impact</button
			>
			{#if store.meta.impactFile}
				<button type="button" class="ghost-btn" onclick={clearImpact}
					>Clear</button
				>
			{/if}
		</div>

		{#if store.error}
			<div class="error-banner">
				<span>{store.error.message}</span>
			</div>
			{#if candidates.length > 0}
				<div class="candidate-panel" aria-label="Symbol candidates">
					<p class="candidate-hint">
						Multiple symbols match this name — pick one:
					</p>
					<ul class="candidate-list">
						{#each candidates as candidate (candidate.stable_id)}
							<li>
								<button
									type="button"
									class="candidate-btn"
									onclick={() => useCandidate(candidate)}
								>
									<span class="candidate-name mono"
										>{candidate.scoped_name}</span
									>
									<span class="candidate-meta"
										>{candidate.kind} · {candidate.file_path}</span
									>
								</button>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}

		{#if isStale}
			<div class="warn-banner">
				<span
					>Relation index advanced to epoch {watchEpoch} (showing {store.meta
						.epoch}).</span
				>
				<button type="button" class="ghost-btn" onclick={runSeed}>Reload</button
				>
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
				{showEdgeLabels}
				onToggleDomain={(domain) => {
					graphActions.toggleDomain(domain);
				}}
				onSearch={(value) => graphActions.setSearch(value)}
				onToggleEdgeLabels={() => (showEdgeLabels = !showEdgeLabels)}
			/>

			<div class="canvas-column">
				<GraphToolbar
					nodeCount={nodes.length}
					edgeCount={edges.length}
					{layout}
					loading={store.loading}
					onZoomIn={() => zoomViewport(0.2)}
					onZoomOut={() => zoomViewport(-0.2)}
					onFit={fitViewport}
					onReset={resetViewport}
					onRelayout={relayout}
					onLayoutChange={(value) => (layout = value)}
					onExportPng={exportPng}
				/>
				<GraphCanvas
					bind:cy
					elements={$graphState.elements}
					{layout}
					focusId={store.meta.focusId}
					visibleDomains={store.filters.domains}
					search={store.filters.search}
					impactDirect={store.meta.impactDirect}
					impactTransitive={store.meta.impactTransitive}
					{showEdgeLabels}
					onNodeSelect={handleNodeSelect}
					onNodeActivate={handleNodeActivate}
				/>
				<p class="canvas-hint">
					Drag to pan · Ctrl/Cmd + wheel to zoom · Double click a node to expand
					its neighborhood
				</p>
			</div>

			<aside class="inspector" aria-label="Node inspector">
				{#if selectedNode}
					<h4 class="inspector-title">{selectedNode.label}</h4>
					<dl class="meta-list">
						<dt>Kind</dt>
						<dd>
							{selectedNode.kind}
							<span class="kind-note"
								>{NODE_KINDS[normalizeNodeKind(selectedNode.kind)]
									.description}</span
							>
						</dd>
						<dt>Location</dt>
						<dd class="mono">
							{selectedNode.source_file}:{selectedNode.source_location}
						</dd>
						<dt>Community</dt>
						<dd>{store.meta.communities[selectedNode.id] ?? '—'}</dd>
					</dl>

					<div class="inspector-actions">
						<button
							type="button"
							class="ghost-btn"
							onclick={() => openEntity(selectedNode.id)}
						>
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
							onclick={() => centerOn(selectedNode.id)}
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
									<span class="relation-domain">{domainLabel(edge)}</span>
								</div>
								<p class="relation-name">{relationLabel(edge.relation)}</p>
								<p class="relation-target mono">
									{outgoing ? edge.target : edge.source}
								</p>
								<p
									class="relation-confidence"
									title={CONFIDENCE_META[confidence].description}
								>
									{CONFIDENCE_META[confidence].label}
								</p>
								{#if edge.cfg_condition}
									<p
										class="relation-guard mono"
										title="Conditional-compilation guard"
									>
										cfg({edge.cfg_condition})
									</p>
								{/if}
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
								onclick={() => loadCommunity(ids)}
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

		<div class="legend">
			{#each Object.values(RELATION_DOMAINS) as domain (domain.domain)}
				<div class="legend-item">
					<span
						class="legend-line"
						style="--line: {domain.color}; --dash: {relationLineStyle(
							domain.domain,
						) === 'solid'
							? '0'
							: '3 2'}"
					></span>
					<span class="legend-label">{domain.label}</span>
				</div>
			{/each}
			<div class="legend-separator">Nodes</div>
			{#each NODE_SHAPE_LEGEND as entry (entry.label)}
				<div class="legend-item">
					<span
						class="legend-node"
						data-shape={entry.shape}
						class:external={entry.external}
					></span>
					<span class="legend-label">{entry.label}</span>
				</div>
			{/each}
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

	.seed-input.narrow {
		min-width: 70px;
		width: 70px;
	}

	.seed-input.wide-input {
		min-width: 320px;
	}

	.file-input {
		min-width: 200px;
	}

	.path-arrow {
		font-family: 'Space Mono', monospace;
		color: var(--gray-500);
	}

	.seed-spacer {
		flex: 1;
	}

	.inline-field {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.inline-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
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

	.candidate-panel {
		padding: 0.6rem 0.75rem;
		margin-bottom: 1rem;
		border: 1px solid var(--gray-300);
	}

	.candidate-hint {
		margin: 0 0 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.candidate-list {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 0.4rem;
		margin: 0;
		padding: 0;
	}

	.candidate-btn {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		width: 100%;
		padding: 0.4rem 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		text-align: left;
		transition: all 0.15s;
	}

	.candidate-btn:hover {
		border-color: var(--black);
		background: var(--gray-100);
	}

	.candidate-name {
		color: var(--black);
	}

	.candidate-meta {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		color: var(--gray-500);
		word-break: break-all;
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

	.relation-guard {
		margin: 0.15rem 0 0;
		font-size: 0.6rem;
		color: var(--gray-400);
		word-break: break-all;
	}

	.kind-note {
		display: block;
		color: var(--gray-400);
		font-size: 0.7rem;
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
		color: var(--gray-400);
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
		border-top: 2px var(--dash, 0) var(--line);
	}

	.legend-separator {
		width: 100%;
		margin-top: 0.35rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-400);
	}

	/* Mirrors the Cytoscape silhouettes closely enough to read as the same
	   vocabulary; a hexagon is the one shape CSS cannot express with a radius,
	   so it is clipped into a polygon instead. */
	.legend-node {
		width: 14px;
		height: 14px;
		border: 1.5px solid var(--black);
		background: var(--white);
		box-sizing: border-box;
	}

	.legend-node[data-shape='rectangle'] {
		border-radius: 0;
	}

	.legend-node[data-shape='round-rectangle'] {
		border-radius: 4px;
	}

	.legend-node[data-shape='hexagon'] {
		clip-path: polygon(25% 0, 75% 0, 100% 50%, 75% 100%, 25% 100%, 0 50%);
	}

	.legend-node[data-shape='diamond'] {
		transform: rotate(45deg) scale(0.82);
		background: var(--white);
	}

	.legend-node.external {
		border-style: dashed;
		border-color: var(--gray-500);
		background: var(--gray-100);
	}

	.legend-node[data-shape='diamond'].external {
		background: var(--gray-100);
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
