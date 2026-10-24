<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { Component } from 'svelte';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { entityState, entityActions } from '$lib/stores/entities';
	import { onProjectChange } from '$lib/stores/project';
	import Card from '$lib/components/ui/Card.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import CallChainList from '$lib/components/entities/CallChainList.svelte';

	// Lazy load all heavy components
	let EntityDetail: Component | null = $state(null);
	let CallGraph: Component | null = $state(null);
	let InheritanceTree: Component | null = $state(null);

	let entityDetailLoaded = $state(false);
	let callGraphLoaded = $state(false);
	let inheritanceTreeLoaded = $state(false);

	let entityId = $derived(page.params.id);
	let currentTab = $state('details');

	// Relation traversal filter shared by calls, callers and call-chain.
	// Every field maps 1:1 onto the request-layer RelationFilter.
	let filterMaxDepth = $state<number | null>(null);
	let filterLimit = $state<number | null>(null);
	let filterExcludeTests = $state(false);
	let filterDirectoryPrefix = $state('');
	let filterDomains = $state('');
	let filterIncludeExternal = $state<'default' | 'on' | 'off'>('default');

	function relationFilter() {
		return {
			maxDepth: filterMaxDepth ?? undefined,
			limit: filterLimit ?? undefined,
			excludeTests: filterExcludeTests || undefined,
			directoryPrefix: filterDirectoryPrefix.trim() || undefined,
			domains: filterDomains.trim() || undefined,
			includeExternal:
				filterIncludeExternal === 'default'
					? undefined
					: filterIncludeExternal === 'on',
		};
	}

	async function loadEntity() {
		if (!entityId) return;
		await entityActions.loadFunction(entityId, relationFilter());
		if (!get(entityState).currentEntity) {
			await entityActions.loadClass(entityId);
		}
	}

	onMount(() => {
		void loadEntity();
	});

	// Entity ids resolve inside one project; after a switch nothing from the
	// previous project may stay on screen, so clear first and reload.
	$effect(() =>
		onProjectChange(() => {
			entityActions.clear();
			void loadEntity();
		}),
	);

	async function loadEntityDetailComponent() {
		if (!entityDetailLoaded) {
			const module =
				await import('$lib/components/entities/EntityDetail.svelte');
			EntityDetail = module.default;
			entityDetailLoaded = true;
		}
	}

	function handleNavigate(id: string) {
		window.location.href = `/entities/${id}`;
	}

	/** Hand the current entity off to the graph explorer, focused on it. */
	function openInGraph(id: string) {
		window.location.href = `/graph?entity=${encodeURIComponent(id)}`;
	}

	function loadCallChain(direction: 'up' | 'down') {
		if (entityId) {
			entityActions.loadCallChain(entityId, direction, relationFilter());
		}
	}

	let pathFrom = $state('');
	let pathTo = $state('');
	let pathDepth = $state(10);

	/** Query the shortest call path between the given function ids. */
	function runCallPath() {
		const from = pathFrom.trim();
		const to = pathTo.trim();
		if (!from || !to) return;
		const depth = Number.isInteger(pathDepth) && pathDepth > 0 ? pathDepth : 10;
		entityActions.loadCallPath(from, to, depth);
	}

	async function loadCallGraphComponent() {
		if (!callGraphLoaded) {
			const module = await import('$lib/components/entities/CallGraph.svelte');
			CallGraph = module.default;
			callGraphLoaded = true;
		}
	}

	async function loadInheritanceTreeComponent() {
		if (!inheritanceTreeLoaded) {
			const module =
				await import('$lib/components/entities/InheritanceTree.svelte');
			InheritanceTree = module.default;
			inheritanceTreeLoaded = true;
		}
	}
</script>

<svelte:head>
	<title>Entity Details - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		{#if $entityState.isLoading}
			<div class="loading-state">
				<p>Loading entity...</p>
			</div>
		{:else if $entityState.error}
			<div class="error-state">
				<p>{$entityState.error}</p>
			</div>
		{:else if $entityState.currentEntity}
			<PageHeader
				title="Entity Details"
				subtitle="Viewing entity: {$entityState.currentEntity.name}"
			/>

			<!-- Relation traversal filter: every control maps onto RelationFilter -->
			<div class="relation-filter" aria-label="Relation filter">
				<label class="filter-field">
					<span>Max depth</span>
					<input
						type="number"
						min="1"
						max="50"
						value={filterMaxDepth ?? ''}
						oninput={(e) => {
							const n = Number((e.target as HTMLInputElement).value);
							filterMaxDepth = Number.isInteger(n) && n > 0 ? n : null;
						}}
						aria-label="Maximum traversal depth"
					/>
				</label>
				<label class="filter-field">
					<span>Limit</span>
					<input
						type="number"
						min="1"
						max="1000"
						value={filterLimit ?? ''}
						oninput={(e) => {
							const n = Number((e.target as HTMLInputElement).value);
							filterLimit = Number.isInteger(n) && n > 0 ? n : null;
						}}
						aria-label="Result limit"
					/>
				</label>
				<label class="filter-field">
					<span>Directory prefix</span>
					<input
						type="text"
						bind:value={filterDirectoryPrefix}
						placeholder="e.g., src/"
						aria-label="Directory prefix"
					/>
				</label>
				<label class="filter-field">
					<span>Domains</span>
					<input
						type="text"
						bind:value={filterDomains}
						placeholder="e.g., call"
						aria-label="Relation domains"
					/>
				</label>
				<label class="filter-check">
					<input type="checkbox" bind:checked={filterExcludeTests} />
					<span>Exclude tests</span>
				</label>
				<label class="filter-field">
					<span>External</span>
					<select
						bind:value={filterIncludeExternal}
						aria-label="Include external edges"
					>
						<option value="default">Default</option>
						<option value="on">Include</option>
						<option value="off">Exclude</option>
					</select>
				</label>
				<button
					type="button"
					class="graph-action-btn"
					onclick={() => void loadEntity()}
				>
					Apply
				</button>
			</div>

			<!-- Tab Navigation -->
			<div class="tab-navigation">
				<button
					class="tab"
					class:active={currentTab === 'details'}
					onclick={() => {
						currentTab = 'details';
						loadEntityDetailComponent();
					}}
				>
					Details
				</button>
				<button
					class="tab"
					class:active={currentTab === 'call-graph'}
					onclick={() => {
						currentTab = 'call-graph';
						loadCallGraphComponent();
						loadCallChain('down');
					}}
				>
					Call Graph
				</button>
				<button
					class="tab"
					class:active={currentTab === 'inheritance'}
					onclick={() => {
						currentTab = 'inheritance';
						loadInheritanceTreeComponent();
					}}
				>
					Inheritance
				</button>
				<button
					class="tab"
					class:active={currentTab === 'call-chain'}
					onclick={() => {
						currentTab = 'call-chain';
						loadCallChain('down');
					}}
				>
					Call Chain
				</button>
				<button
					class="tab"
					class:active={currentTab === 'call-path'}
					onclick={() => {
						currentTab = 'call-path';
					}}
				>
					Call Path
				</button>
			</div>

			<!-- Tab Content -->
			{#if currentTab === 'details'}
				{#if entityDetailLoaded && EntityDetail}
					<EntityDetail
						func={$entityState.currentEntity}
						calls={$entityState.calls?.callees ?? []}
						callers={$entityState.callers?.callers ?? []}
						onNavigate={handleNavigate}
					/>
				{:else}
					<button
						class="loading-spinner-button"
						onclick={loadEntityDetailComponent}
						aria-label="Load entity details"
					>
						Loading entity details... (click to load)
					</button>
				{/if}
			{:else if currentTab === 'call-graph'}
				<Card title="Call Graph" subtitle="Visual relationship map">
					<div class="graph-actions">
						<button
							type="button"
							class="graph-action-btn"
							onclick={() => openInGraph(String(entityId))}
						>
							Open in Graph Explorer
						</button>
					</div>
					{#if CallGraph}
						<CallGraph
							nodes={$entityState.callChain}
							focusId={String(entityId)}
							onNavigate={handleNavigate}
						/>
					{:else}
						<LoadingState message="Loading graph..." />
					{/if}
				</Card>
			{:else if currentTab === 'inheritance'}
				<Card title="Inheritance Tree" subtitle="Class hierarchy">
					{#if InheritanceTree}
						<InheritanceTree
							inheritance={$entityState.inheritance}
							implementations={$entityState.implementations}
							focusId={String(entityId)}
							fallbackName={$entityState.currentEntity?.name ?? null}
							onNavigate={handleNavigate}
						/>
					{:else}
						<LoadingState message="Loading tree..." />
					{/if}
				</Card>
			{:else if currentTab === 'call-chain'}
				<Card title="Call Chain" subtitle="Linear execution path">
					{#if $entityState.callChain.length > 0}
						<CallChainList nodes={$entityState.callChain} />
					{:else}
						<p class="placeholder-text">No call chain data available</p>
					{/if}
				</Card>
			{:else if currentTab === 'call-path'}
				<Card
					title="Two-point Call Path"
					subtitle="Shortest call path between two functions"
				>
					<div class="path-form">
						<input
							class="path-input"
							type="text"
							placeholder="From function id…"
							bind:value={pathFrom}
							aria-label="Path start function id"
						/>
						<span class="path-arrow" aria-hidden="true"><Icon name="arrow-right" /></span>
						<input
							class="path-input"
							type="text"
							placeholder="To function id…"
							bind:value={pathTo}
							aria-label="Path end function id"
						/>
						<input
							class="path-input path-depth"
							type="number"
							min="1"
							max="100"
							bind:value={pathDepth}
							aria-label="Path maximum search depth"
						/>
						<button
							type="button"
							class="graph-action-btn"
							onclick={runCallPath}
							disabled={$entityState.isLoading}
						>
							{$entityState.isLoading ? 'Searching...' : 'Find Path'}
						</button>
					</div>

					{#if $entityState.error}
						<p class="path-error">{$entityState.error}</p>
					{/if}

					{#if $entityState.callPath.length > 0}
						<CallChainList nodes={$entityState.callPath} />
						<p class="path-meta">{$entityState.callPathLength} hop(s)</p>
					{:else if !$entityState.callPathFound && !$entityState.isLoading}
						<p class="placeholder-text">
							No call path found between the two functions.
						</p>
					{/if}
				</Card>
			{/if}
		{:else}
			<div class="not-found">
				<h2>Entity Not Found</h2>
				<p>No entity found with ID: {entityId}</p>
			</div>
		{/if}
	</div>
</div>

<style>
	.loading-state,
	.error-state,
	.not-found {
		padding: 4rem 2rem;
		text-align: center;
		border: 1px solid var(--gray-200);
	}

	.loading-state p,
	.error-state p {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}

	.error-state {
		border-color: var(--danger);
		color: var(--danger);
	}

	.not-found h2 {
		font-size: 1.5rem;
		margin-bottom: 1rem;
	}

	.tab-navigation {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 0.5rem;
		margin-bottom: 2rem;
		border-bottom: 1px solid var(--gray-200);
		padding-bottom: 1rem;
	}

	.tab {
		padding: 0.75rem 1rem;
		background: var(--white);
		border: 1px solid var(--gray-200);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: all 0.2s;
	}

	.tab:hover {
		background: var(--gray-100);
	}

	.tab.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
	}

	.relation-filter {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: end;
		margin-bottom: 1.5rem;
		padding: 0.75rem;
		border: 1px solid var(--gray-200);
		background: var(--gray-50);
	}

	.filter-field {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	.filter-field input,
	.filter-field select {
		height: 30px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		min-width: 110px;
	}

	.filter-check {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
		height: 30px;
	}

	.graph-actions {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 1rem;
	}

	.graph-action-btn {
		padding: 0.5rem 0.85rem;
		background: var(--black);
		color: var(--white);
		border: 1px solid var(--black);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: all 0.15s;
	}

	.graph-action-btn:hover {
		background: var(--gray-900);
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 2rem;
	}

	.loading-spinner-button {
		padding: 2rem;
		text-align: center;
		color: var(--gray-600);
		font-family: 'Space Mono', monospace;
		background: none;
		border: 1px dashed var(--gray-300);
		width: 100%;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.loading-spinner-button:hover {
		border-color: var(--accent);
		color: var(--black);
		background: var(--gray-50);
	}
	.path-form {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}

	.path-input {
		height: 32px;
		min-width: 200px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.path-input:focus {
		outline: none;
		border-color: var(--black);
	}

	.path-input.path-depth {
		min-width: 70px;
		width: 70px;
	}

	.path-arrow {
		color: var(--gray-500);
	}

	.path-error {
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		margin: 0.5rem 0;
	}

	.path-meta {
		margin: 0.75rem 0 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.68rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}
</style>
