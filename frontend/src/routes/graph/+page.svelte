<script lang="ts">
	/**
	 * Graph explorer page.
	 *
	 * Composes the canvas, viewport toolbar, filter panel and the feature
	 * panels (seed bar, node inspector, communities, cycles, module
	 * relations, legend) over a single shared graph store. The page owns the
	 * seed strategy (an entity oriented load, an explicit subgraph, a path
	 * search or a bounded project overview) and cross-panel orchestration;
	 * each panel owns its own form state, results and styles. The store owns
	 * accumulation and cache invalidation.
	 *
	 * The Cytoscape Core instance is owned directly by this page via bind:cy.
	 * All imperative viewport operations (zoom/fit/relayout/export) go
	 * through the free functions in `$lib/utils/graph-viewport`.
	 */
	import { onDestroy, onMount } from 'svelte';
	import type { Core } from 'cytoscape';
	import { page } from '$app/state';
	import { SvelteMap } from 'svelte/reactivity';
	import { watchStore } from '$lib/stores/watch.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
	import GraphCanvas, {
		layoutOptions,
		type GraphLayoutName,
	} from '$lib/components/graph/GraphCanvas.svelte';
	import GraphToolbar from '$lib/components/graph/GraphToolbar.svelte';
	import { graphStore, activeDomains } from '$lib/stores/graph.svelte';
	import { StaleQuery } from '$lib/stores/stale-query.svelte';
	import { onProjectChange } from '$lib/stores/project.svelte';
	import type { GraphDirection, GraphModuleResponse } from '$lib/api/graph';
	import type { SymbolCandidate } from '$lib/api/client';
	import {
		buildServerFilter,
		clampOverviewLimit,
		clampPositive,
		parseEntityIds,
	} from '$lib/utils/graph-params';
	import {
		centerGraphOn,
		exportGraphPng,
		fitGraphViewport,
		resetGraphViewport,
		zoomGraphViewport,
	} from '$lib/utils/graph-viewport';

	let cy = $state<Core | null>(null);
	let layout: GraphLayoutName = $state('cose');
	let resultTab: 'communities' | 'cycles' | 'module' = $state('communities');

	// Secondary panels load after the canvas mounts so the explorer's first
	// paint is dominated by the renderer, not the surrounding forms.
	type ComponentOf<T> = T extends { default: infer C } ? C : never;
	type GraphSeedBarC = ComponentOf<
		typeof import('$lib/components/graph/GraphSeedBar.svelte')
	>;
	type GraphFilterPanelC = ComponentOf<
		typeof import('$lib/components/graph/GraphFilterPanel.svelte')
	>;
	type GraphLegendC = ComponentOf<
		typeof import('$lib/components/graph/GraphLegend.svelte')
	>;
	type GraphCandidatePanelC = ComponentOf<
		typeof import('$lib/components/graph/GraphCandidatePanel.svelte')
	>;
	type GraphNodeInspectorC = ComponentOf<
		typeof import('$lib/components/graph/GraphNodeInspector.svelte')
	>;
	type GraphCommunitiesPanelC = ComponentOf<
		typeof import('$lib/components/graph/GraphCommunitiesPanel.svelte')
	>;
	type GraphCyclesPanelC = ComponentOf<
		typeof import('$lib/components/graph/GraphCyclesPanel.svelte')
	>;
	type GraphModulePanelC = ComponentOf<
		typeof import('$lib/components/graph/GraphModulePanel.svelte')
	>;

	let GraphSeedBar: GraphSeedBarC | null = $state(null);
	let GraphFilterPanel: GraphFilterPanelC | null = $state(null);
	let GraphLegend: GraphLegendC | null = $state(null);
	let GraphCandidatePanel: GraphCandidatePanelC | null = $state(null);
	let GraphNodeInspector: GraphNodeInspectorC | null = $state(null);
	let GraphCommunitiesPanel: GraphCommunitiesPanelC | null = $state(null);
	let GraphCyclesPanel: GraphCyclesPanelC | null = $state(null);
	let GraphModulePanel: GraphModulePanelC | null = $state(null);
	let activePanelsLoaded = $state(false);

	/** Load the always-visible panels plus the default result tab. */
	async function loadActivePanels() {
		if (activePanelsLoaded) return;
		activePanelsLoaded = true;
		const [seed, filter, legend, inspector, communities, candidatePanel] =
			await Promise.all([
				import('$lib/components/graph/GraphSeedBar.svelte'),
				import('$lib/components/graph/GraphFilterPanel.svelte'),
				import('$lib/components/graph/GraphLegend.svelte'),
				import('$lib/components/graph/GraphNodeInspector.svelte'),
				import('$lib/components/graph/GraphCommunitiesPanel.svelte'),
				import('$lib/components/graph/GraphCandidatePanel.svelte'),
			]);
		GraphSeedBar = seed.default;
		GraphFilterPanel = filter.default;
		GraphLegend = legend.default;
		GraphNodeInspector = inspector.default;
		GraphCommunitiesPanel = communities.default;
		GraphCandidatePanel = candidatePanel.default;
	}

	/** Load a result-tab panel the first time its tab becomes active. */
	async function loadResultTab(tab: 'communities' | 'cycles' | 'module') {
		resultTab = tab;
		if (tab === 'communities') {
			if (!GraphCommunitiesPanel)
				GraphCommunitiesPanel = (
					await import('$lib/components/graph/GraphCommunitiesPanel.svelte')
				).default;
		} else if (tab === 'cycles') {
			if (!GraphCyclesPanel)
				GraphCyclesPanel = (
					await import('$lib/components/graph/GraphCyclesPanel.svelte')
				).default;
		} else if (!GraphModulePanel) {
			GraphModulePanel = (
				await import('$lib/components/graph/GraphModulePanel.svelte')
			).default;
		}
	}
	let seedMode: 'focus' | 'overview' | 'path' | 'subgraph' = $state('focus');
	let seedId = $state('');
	let selectedId: string | null = $state(null);
	let impactFile = $state('');
	let impactMode = $state<'file' | 'entity'>('file');
	let impactEntity = $state('');
	let impactScope = $state<'entity' | 'file'>('entity');
	let impactDepth = $state(10);
	let impactBusy = $state(false);
	let impactMessage = $state<string | null>(null);

	let pathStart = $state('');
	let pathEnd = $state('');
	let pathDepth = $state(10);
	let subgraphIds = $state('');
	let egoDepth = $state(2);
	let egoDirection = $state<GraphDirection>('both');
	let showEdgeLabels = $state(false);

	// Server-side traversal filter, applied only when a seed load runs. Domain
	// visibility stays purely client-side: hidden-domain edges must remain in
	// the working set so incremental expansion keeps its full context.
	let serverRelationTypes = $state('');
	let serverIncludeExternal = $state<'default' | 'on' | 'off'>('default');
	let overviewLimit = $state(400);

	// The query string may carry an entity to focus on, which is how the entity
	// detail page hands off to the explorer.
	let queryEntity = $derived(page.url.searchParams.get('entity') ?? '');
	let queryFile = $derived(page.url.searchParams.get('file') ?? '');

	let store = $derived(graphStore);
	let nodes = $derived(store.nodes);
	let edges = $derived(store.edges);
	let availableDomains = $derived([...activeDomains(edges)]);
	let searchNeedle = $derived(store.filters.search.trim().toLowerCase());
	let searchMatchCount = $derived(
		searchNeedle.length >= 2
			? nodes.filter((node) =>
					String(node.label ?? '')
						.toLowerCase()
						.includes(searchNeedle),
				).length
			: 0,
	);

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
			selectedId = queryEntity;
		} else {
			// Nothing seeded from the URL: show the whole project graph and keep
			// the seed mode in sync so a project switch re-loads the same view.
			seedMode = 'overview';
		}
		if (queryFile) {
			impactFile = queryFile;
		}
		await runSeed();
		if (queryFile) {
			await graphStore.loadImpact(queryFile);
		}
	}

	onMount(async () => {
		void loadActivePanels();
		await loadInitialGraph();
		await watchStore.loadStatus();
		watchStore.startVersionPoll();
	});

	onDestroy(() => {
		watchStore.stopVersionPoll();
	});

	// The canvas only ever holds one project's nodes: clear the previous working
	// set and re-run the current seed against the newly selected project.
	$effect(() =>
		onProjectChange((projectId) => {
			graphStore.reset(projectId);
			void runSeed();
		}),
	);

	function serverFilter() {
		return buildServerFilter(serverRelationTypes, serverIncludeExternal);
	}

	async function runSeed() {
		const id = seedId.trim();
		const filter = serverFilter();
		if (seedMode === 'focus') {
			if (!id) return;
			selectedId = id;
			await graphStore.loadEgo(id, egoDepth, egoDirection, undefined, filter);
		} else if (seedMode === 'path') {
			const start = pathStart.trim();
			const end = pathEnd.trim();
			if (!start || !end) return;
			await graphStore.loadPath(
				start,
				end,
				clampPositive(pathDepth, 10, 100),
				undefined,
				filter,
			);
		} else if (seedMode === 'subgraph') {
			const ids = parseEntityIds(subgraphIds);
			if (ids.length === 0) return;
			await graphStore.loadSubgraph(ids, undefined, filter);
		} else {
			await graphStore.loadOverview(
				clampOverviewLimit(overviewLimit),
				undefined,
				filter,
			);
		}
		await graphStore.loadComponentsIfStale(undefined, filter);
	}

	// Disambiguation candidates shown when a symbol-name seed matches multiple
	// entities (backend AMBIGUOUS_SYMBOL). Picking one retries with its stable id.
	let candidates: SymbolCandidate[] = $derived(store.error?.candidates ?? []);

	// The backend advanced past the epoch this working set was built from.
	// Banner only: reload stays explicit so in-progress expansion is never lost;
	// reloading rebuilds the current view from the seed and discards expansion.
	let watchEpoch = $derived(watchStore.status?.relation_epoch ?? null);
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
		await graphStore.expand(nodeId, 1, egoDirection, serverFilter());
		cy?.layout(layoutOptions(layout)).run();
	}

	function handleNodeSelect(nodeId: string) {
		selectedId = nodeId;
	}

	async function runImpact() {
		if (impactBusy) return;
		impactBusy = true;
		impactMessage = null;
		try {
			if (impactMode === 'entity') {
				const entityId = impactEntity.trim();
				if (!entityId) {
					graphStore.clearImpact();
					return;
				}
				const depth = clampPositive(impactDepth, 10, 50);
				const result = await graphStore.loadEntityImpact({
					entityId,
					maxDepth: depth,
					scope: impactScope,
				});
				if (result.ok) {
					const direct = graphStore.meta.impactDirect.length;
					const transitive = graphStore.meta.impactTransitive.length;
					impactMessage =
						direct + transitive === 0
							? `No dependents found for '${entityId}'.`
							: `Entity '${entityId}': ${direct} direct, ${transitive} indirect dependents.`;
				}
			} else {
				const file = impactFile.trim();
				if (!file) {
					graphStore.clearImpact();
					return;
				}
				const result = await graphStore.loadImpact(file);
				if (result.ok) {
					const direct = graphStore.meta.impactDirect.length;
					const transitive = graphStore.meta.impactTransitive.length;
					impactMessage =
						direct + transitive === 0
							? `No dependents found for '${file}'.`
							: `File '${file}': ${direct} direct, ${transitive} indirect dependents.`;
				}
			}
		} finally {
			impactBusy = false;
		}
	}

	function clearImpact() {
		impactFile = '';
		impactEntity = '';
		impactMessage = null;
		graphStore.clearImpact();
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

	// Module-level relations for one file. Shares the file input with impact
	// analysis and stays page-local like cycles and structural results.
	let moduleQuery = new StaleQuery<GraphModuleResponse>(
		() => watchStore.status?.relation_epoch ?? null,
	);

	async function runModule() {
		const file = impactFile.trim();
		if (!file) {
			moduleQuery.error = 'Enter a file path to query module relations.';
			return;
		}
		await moduleQuery.run(() => graphStore.loadModule(file));
	}

	/** Focus an entity picked anywhere in the UI (module exports, peers). */
	async function focusPeer(entityId: string) {
		seedMode = 'focus';
		seedId = entityId;
		selectedId = entityId;
		await graphStore.loadEgo(
			entityId,
			egoDepth,
			egoDirection,
			undefined,
			serverFilter(),
		);
		await graphStore.loadComponentsIfStale(undefined, serverFilter());
	}

	async function impactCallerFile(file: string) {
		impactMode = 'file';
		impactFile = file;
		await runImpact();
	}

	/** Load a set of ids as an induced subgraph and re-run the layout. */
	async function openAsSubgraph(ids: string[]) {
		await graphStore.loadSubgraph(ids, undefined, serverFilter());
		await graphStore.loadComponentsIfStale(undefined, serverFilter());
		cy?.layout(layoutOptions(layout)).run();
	}

	function relayout() {
		cy?.layout(layoutOptions(layout)).run();
	}

	function focusSearchMatches() {
		if (!cy) return;
		const needle = searchNeedle;
		if (needle.length < 2) return;
		const matched = nodes.filter((node) =>
			String(node.label ?? '')
				.toLowerCase()
				.includes(needle),
		);
		if (matched.length === 0) return;
		const eles = cy.collection();
		for (const node of matched) {
			const el = cy.getElementById(node.id);
			if (el.length > 0) eles.merge(el);
		}
		if (eles.length > 0) cy.fit(eles, 60);
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

		{#if GraphSeedBar}
			<GraphSeedBar
				bind:seedMode
				bind:seedId
				bind:pathStart
				bind:pathEnd
				bind:pathDepth
				bind:subgraphIds
				bind:overviewLimit
				bind:egoDepth
				bind:egoDirection
				bind:impactFile
				bind:impactMode
				bind:impactEntity
				bind:impactScope
				bind:impactDepth
				bind:serverRelationTypes
				bind:serverIncludeExternal
				loading={store.loading}
				{impactBusy}
				moduleLoading={moduleQuery.loading}
				showClear={Boolean(store.meta.impactFile)}
				onRunSeed={runSeed}
				onRunImpact={runImpact}
				onRunModule={runModule}
				onClearImpact={clearImpact}
			/>
		{/if}

		{#if impactMessage}
			<Notice variant="info" label="Impact result" message={impactMessage} />
		{/if}

		{#if store.meta.impactDirect.length || store.meta.impactTransitive.length}
			<div class="impact-dependents">
				{#if store.meta.impactDirect.length}
					<details open>
						<summary
							>Direct dependents ({store.meta.impactDirect.length})</summary
						>
						<ul class="impact-list">
							{#each store.meta.impactDirect as id (id)}
								<li>
									<Button
										variant="ghost"
										size="xs"
										onclick={() => {
											selectedId = id;
											centerGraphOn(cy, id);
										}}>{id}</Button
									>
								</li>
							{/each}
						</ul>
					</details>
				{/if}
				{#if store.meta.impactTransitive.length}
					<details>
						<summary
							>Indirect dependents ({store.meta.impactTransitive
								.length})</summary
						>
						<ul class="impact-list">
							{#each store.meta.impactTransitive as id (id)}
								<li>
									<Button
										variant="ghost"
										size="xs"
										onclick={() => {
											selectedId = id;
											centerGraphOn(cy, id);
										}}>{id}</Button
									>
								</li>
							{/each}
						</ul>
					</details>
				{/if}
			</div>
		{/if}

		{#if store.error}
			<ErrorBanner message={store.error.message} />
			{#if candidates.length > 0 && GraphCandidatePanel}
				<GraphCandidatePanel {candidates} onPick={useCandidate} />
			{/if}
		{/if}

		{#if isStale}
			<Notice variant="warning">
				Relation index advanced to epoch {watchEpoch} (showing {store.meta
					.epoch}). Reload rebuilds the current view from the seed.
				{#snippet action()}
					<Button variant="ghost" onclick={runSeed}>Reload</Button>
				{/snippet}
			</Notice>
		{/if}

		{#if store.truncated}
			<Notice variant="warning">
				Node limit reached. Expand is disabled until you reload a smaller seed.
			</Notice>
		{/if}

		<div class="workspace">
			<div class="filter-column">
				{#if GraphFilterPanel}
					<GraphFilterPanel
						domains={store.filters.domains}
						{availableDomains}
						search={store.filters.search}
						{showEdgeLabels}
						{searchMatchCount}
						onToggleDomain={(domain) => {
							graphStore.toggleDomain(domain);
						}}
						onSearch={(value) => graphStore.setSearch(value)}
						onToggleEdgeLabels={() => (showEdgeLabels = !showEdgeLabels)}
						onFocusMatches={focusSearchMatches}
					/>
				{/if}
			</div>

			<div class="canvas-column">
				<GraphToolbar
					nodeCount={nodes.length}
					edgeCount={edges.length}
					{layout}
					loading={store.loading}
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
					elements={graphStore.elements}
					{layout}
					loading={store.loading}
					focusId={store.meta.focusId}
					{selectedId}
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
				<details class="canvas-help">
					<summary>Navigation help</summary>
					<ul class="canvas-help-list">
						<li>Drag the canvas to pan; Ctrl/Cmd + wheel to zoom.</li>
						<li>Click a node to inspect it; double click to expand one hop.</li>
						<li>Use + / - / 0 on the canvas for zoom in, zoom out and fit.</li>
						<li>Press Enter on a focused node to expand its neighborhood.</li>
					</ul>
				</details>
				{#if GraphLegend}
					<div class="canvas-legend">
						<GraphLegend />
					</div>
				{/if}
			</div>

			{#if GraphNodeInspector}
				<GraphNodeInspector
					node={selectedNode}
					edges={selectedEdges}
					communities={store.meta.communities}
					onExpand={handleNodeActivate}
					onCenter={(nodeId) => centerGraphOn(cy, nodeId)}
					onFocusEntity={focusPeer}
				/>
			{/if}
		</div>

		<details class="result-section" open>
			<summary class="result-section-summary">Results</summary>
			<Tabs
				variant="segmented"
				label="Graph results"
				active={resultTab}
				tabs={[
					{ id: 'communities', label: 'Communities' },
					{ id: 'cycles', label: 'Cycles' },
					{ id: 'module', label: 'Module' },
				]}
				onSelect={(id) => loadResultTab(id as typeof resultTab)}
			/>

			{#if resultTab === 'communities' && GraphCommunitiesPanel}
				<GraphCommunitiesPanel {communities} onLoad={openAsSubgraph} />
			{:else if resultTab === 'cycles' && GraphCyclesPanel}
				<GraphCyclesPanel onLoadSubgraph={openAsSubgraph} />
			{:else if resultTab === 'module' && GraphModulePanel}
				<GraphModulePanel
					data={moduleQuery.data}
					loading={moduleQuery.loading}
					error={moduleQuery.error}
					stale={moduleQuery.stale}
					{watchEpoch}
					onFocusEntity={focusPeer}
					onImpactFile={impactCallerFile}
				/>
			{/if}
		</details>
	</div>
</div>

<style>
	.workspace {
		display: grid;
		grid-template-columns: minmax(200px, 240px) minmax(0, 1fr) minmax(
				240px,
				300px
			);
		gap: 1rem;
		align-items: start;
	}

	.filter-column {
		display: flex;
		flex-direction: column;
		gap: 1rem;
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

	.impact-dependents {
		margin-bottom: 1rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--gray-300);
		background: var(--gray-50);
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.impact-dependents details {
		margin: 0.25rem 0;
	}

	.impact-dependents summary {
		cursor: pointer;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
	}

	.impact-list {
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		margin: 0.4rem 0 0;
		padding: 0;
	}

	.canvas-help {
		margin: 0.5rem 0 0;
		border: 1px solid var(--gray-300);
		padding: 0.35rem 0.6rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--gray-600);
	}

	.canvas-help summary {
		cursor: pointer;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.canvas-help-list {
		margin: 0.4rem 0 0;
		padding-left: 1.1rem;
		color: var(--gray-600);
	}

	.canvas-legend {
		margin-top: 0.75rem;
		border: 1px solid var(--gray-300);
		padding: 0.5rem;
	}

	.result-section {
		margin-top: 1rem;
		border: 1px solid var(--black);
	}

	.result-section-summary {
		cursor: pointer;
		padding: 0.5rem 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
		background: var(--gray-50);
	}

	@media (max-width: 1200px) {
		.workspace {
			grid-template-columns: 1fr;
		}

		.canvas-column {
			position: sticky;
			top: 0;
			z-index: 2;
		}
	}
</style>
