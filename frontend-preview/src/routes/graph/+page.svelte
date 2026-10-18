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
	import { watchState, watchActions } from '$lib/stores/watch';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import GraphCanvas, {
		layoutOptions,
		type GraphLayoutName,
	} from '$lib/components/graph/GraphCanvas.svelte';
	import GraphToolbar from '$lib/components/graph/GraphToolbar.svelte';
	import GraphFilterPanel from '$lib/components/graph/GraphFilterPanel.svelte';
	import GraphSeedBar from '$lib/components/graph/GraphSeedBar.svelte';
	import GraphCandidatePanel from '$lib/components/graph/GraphCandidatePanel.svelte';
	import GraphNodeInspector from '$lib/components/graph/GraphNodeInspector.svelte';
	import GraphCommunitiesPanel from '$lib/components/graph/GraphCommunitiesPanel.svelte';
	import GraphCyclesPanel from '$lib/components/graph/GraphCyclesPanel.svelte';
	import GraphModulePanel from '$lib/components/graph/GraphModulePanel.svelte';
	import GraphLegend from '$lib/components/graph/GraphLegend.svelte';
	import { graphState, graphActions, activeDomains } from '$lib/stores/graph';
	import { StaleQuery } from '$lib/stores/stale-query.svelte';
	import { onProjectChange } from '$lib/stores/project';
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
	let layout: GraphLayoutName = $state('cose-bilkent');
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
			await graphActions.loadEgo(
				queryEntity,
				egoDepth,
				egoDirection,
				undefined,
				serverFilter(),
			);
			selectedId = queryEntity;
		} else {
			// Nothing seeded from the URL: show the whole project graph and keep
			// the seed mode in sync so a project switch re-loads the same view.
			seedMode = 'overview';
			await graphActions.loadOverview(
				clampOverviewLimit(overviewLimit),
				undefined,
				serverFilter(),
			);
		}
		if (queryFile) {
			impactFile = queryFile;
			await graphActions.loadImpact(queryFile);
		}
		await graphActions.loadComponentsIfStale(undefined, serverFilter());
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

	function serverFilter() {
		return buildServerFilter(serverRelationTypes, serverIncludeExternal);
	}

	async function runSeed() {
		const id = seedId.trim();
		const filter = serverFilter();
		if (seedMode === 'focus') {
			if (!id) return;
			selectedId = id;
			await graphActions.loadEgo(id, egoDepth, egoDirection, undefined, filter);
		} else if (seedMode === 'path') {
			const start = pathStart.trim();
			const end = pathEnd.trim();
			if (!start || !end) return;
			await graphActions.loadPath(
				start,
				end,
				clampPositive(pathDepth, 10, 100),
				undefined,
				filter,
			);
		} else if (seedMode === 'subgraph') {
			const ids = parseEntityIds(subgraphIds);
			if (ids.length === 0) return;
			await graphActions.loadSubgraph(ids, undefined, filter);
		} else {
			await graphActions.loadOverview(
				clampOverviewLimit(overviewLimit),
				undefined,
				filter,
			);
		}
		await graphActions.loadComponentsIfStale(undefined, filter);
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
		await graphActions.expand(nodeId, 1, egoDirection, serverFilter());
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
					graphActions.clearImpact();
					return;
				}
				const depth = clampPositive(impactDepth, 10, 50);
				const result = await graphActions.loadEntityImpact({
					entityId,
					maxDepth: depth,
					scope: impactScope,
				});
				if (result.ok) {
					const direct = $graphState.meta.impactDirect.length;
					const transitive = $graphState.meta.impactTransitive.length;
					impactMessage =
						direct + transitive === 0
							? `No dependents found for '${entityId}'.`
							: `Entity '${entityId}': ${direct} direct, ${transitive} indirect dependents.`;
				}
			} else {
				const file = impactFile.trim();
				if (!file) {
					graphActions.clearImpact();
					return;
				}
				const result = await graphActions.loadImpact(file);
				if (result.ok) {
					const direct = $graphState.meta.impactDirect.length;
					const transitive = $graphState.meta.impactTransitive.length;
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
		graphActions.clearImpact();
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
		() => $watchState.status?.relation_epoch ?? null,
	);

	async function runModule() {
		const file = impactFile.trim();
		if (!file) {
			moduleQuery.error = 'Enter a file path to query module relations.';
			return;
		}
		await moduleQuery.run(() => graphActions.loadModule(file));
	}

	/** Focus an entity picked anywhere in the UI (module exports, peers). */
	async function focusPeer(entityId: string) {
		seedMode = 'focus';
		seedId = entityId;
		selectedId = entityId;
		await graphActions.loadEgo(
			entityId,
			egoDepth,
			egoDirection,
			undefined,
			serverFilter(),
		);
		await graphActions.loadComponentsIfStale(undefined, serverFilter());
	}

	async function impactCallerFile(file: string) {
		impactMode = 'file';
		impactFile = file;
		await runImpact();
	}

	/** Load a set of ids as an induced subgraph and re-run the layout. */
	async function openAsSubgraph(ids: string[]) {
		await graphActions.loadSubgraph(ids);
		cy?.layout(layoutOptions(layout)).run();
	}

	function relayout() {
		cy?.layout(layoutOptions(layout)).run();
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

		{#if impactMessage}
			<div class="info-banner" aria-label="Impact result">
				<span>{impactMessage}</span>
			</div>
		{/if}

		{#if store.error}
			<div class="error-banner">
				<span>{store.error.message}</span>
			</div>
			{#if candidates.length > 0}
				<GraphCandidatePanel {candidates} onPick={useCandidate} />
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
					elements={$graphState.elements}
					{layout}
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
			</div>

			<GraphNodeInspector
				node={selectedNode}
				edges={selectedEdges}
				communities={store.meta.communities}
				onExpand={handleNodeActivate}
				onCenter={(nodeId) => centerGraphOn(cy, nodeId)}
				onFocusEntity={focusPeer}
			/>
		</div>

		<GraphCommunitiesPanel {communities} onLoad={openAsSubgraph} />

		<GraphCyclesPanel onLoadSubgraph={openAsSubgraph} />

		<GraphModulePanel
			data={moduleQuery.data}
			loading={moduleQuery.loading}
			error={moduleQuery.error}
			stale={moduleQuery.stale}
			{watchEpoch}
			onFocusEntity={focusPeer}
			onImpactFile={impactCallerFile}
		/>

		<GraphLegend />
	</div>
</div>

<style>
	.info-banner,
	.warn-banner,
	.error-banner {
		padding: 0.6rem 0.75rem;
		margin-bottom: 1rem;
		border: 1px solid;
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
	}

	.info-banner {
		border-color: var(--gray-300);
		background: var(--gray-100);
		color: var(--gray-600);
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
		background: var(--white);
		color: var(--black);
	}

	.ghost-btn:hover {
		background: var(--gray-100);
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

	@media (max-width: 1200px) {
		.workspace {
			grid-template-columns: 1fr;
		}
	}
</style>
