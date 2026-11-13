<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { entityStore } from '$lib/stores/entities.svelte';
	import { onProjectChange } from '$lib/stores/project.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import LazyPanel from '$lib/components/ui/LazyPanel.svelte';
	import SymbolCandidatePanel from '$lib/components/ui/SymbolCandidatePanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import CallChainList from '$lib/components/entities/CallChainList.svelte';

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

	interface AppliedFilter {
		maxDepth: number | null;
		limit: number | null;
		excludeTests: boolean;
		directoryPrefix: string;
		domains: string;
		includeExternal: 'default' | 'on' | 'off';
	}

	const DEFAULT_FILTER: AppliedFilter = {
		maxDepth: null,
		limit: null,
		excludeTests: false,
		directoryPrefix: '',
		domains: '',
		includeExternal: 'default',
	};

	let applied = $state<AppliedFilter>({ ...DEFAULT_FILTER });

	function toRequestFilter(state: AppliedFilter) {
		return {
			maxDepth: state.maxDepth ?? undefined,
			limit: state.limit ?? undefined,
			excludeTests: state.excludeTests || undefined,
			directoryPrefix: state.directoryPrefix.trim() || undefined,
			domains: state.domains.trim() || undefined,
			includeExternal:
				state.includeExternal === 'default'
					? undefined
					: state.includeExternal === 'on',
		};
	}

	/** Request filter built from the last applied controls, not the draft. */
	function relationFilter() {
		return toRequestFilter(applied);
	}

	function draftFilter(): AppliedFilter {
		return {
			maxDepth: filterMaxDepth,
			limit: filterLimit,
			excludeTests: filterExcludeTests,
			directoryPrefix: filterDirectoryPrefix,
			domains: filterDomains,
			includeExternal: filterIncludeExternal,
		};
	}

	let filterDirty = $derived(
		JSON.stringify(draftFilter()) !== JSON.stringify(applied),
	);

	function applyFilters() {
		applied = draftFilter();
		void loadEntity();
	}

	async function loadEntity() {
		if (!entityId) return;
		await entityStore.loadFunction(entityId, relationFilter());
		if (!entityStore.currentEntity) {
			await entityStore.loadClass(entityId);
		}
	}

	onMount(() => {
		void loadEntity();
	});

	// Entity ids resolve inside one project; after a switch nothing from the
	// previous project may stay on screen, so clear first and reload.
	$effect(() =>
		onProjectChange(() => {
			entityStore.clear();
			void loadEntity();
		}),
	);

	function handleNavigate(id: string) {
		goto(resolve(`/entities/${id}`));
	}

	/** Hand the current entity off to the graph explorer, focused on it. */
	function openInGraph(id: string) {
		goto(resolve(`/graph?entity=${encodeURIComponent(id)}`));
	}

	function loadCallChain(direction: 'up' | 'down') {
		if (entityId) {
			entityStore.loadCallChain(entityId, direction, relationFilter());
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
		entityStore.loadCallPath(from, to, depth);
	}
</script>

<svelte:head>
	<title>Entity Details - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		{#if entityStore.isLoading}
			<LoadingState message="Loading entity..." />
		{:else if entityStore.error}
			<ErrorBanner
				message={entityStore.error.message}
				code={entityStore.error.code}
			/>
			{#if entityStore.error.candidates.length > 0}
				<SymbolCandidatePanel
					candidates={entityStore.error.candidates}
					onPick={(candidate) =>
						goto(resolve(`/entities/${candidate.stable_id}`))}
				/>
			{/if}
		{:else if entityStore.currentEntity}
			<PageHeader
				title="Entity Details"
				subtitle="Viewing entity: {entityStore.currentEntity.name}"
			/>

			<!-- Relation traversal filter: every control maps onto RelationFilter -->
			<div class="relation-filter" aria-label="Relation filter">
				<label class="graph-inline-field">
					<span class="graph-inline-label">Max depth</span>
					<input
						class="text-input text-input--narrow"
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
				<label class="graph-inline-field">
					<span class="graph-inline-label">Limit</span>
					<input
						class="text-input text-input--narrow"
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
				<label class="graph-inline-field">
					<span class="graph-inline-label">Directory prefix</span>
					<input
						class="text-input text-input--inline"
						type="text"
						bind:value={filterDirectoryPrefix}
						placeholder="e.g., src/"
						aria-label="Directory prefix"
					/>
				</label>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Domains</span>
					<input
						class="text-input text-input--inline"
						type="text"
						bind:value={filterDomains}
						placeholder="e.g., call"
						aria-label="Relation domains"
					/>
				</label>
				<Toggle
					checked={filterExcludeTests}
					label="Exclude tests"
					onchange={(e) => (filterExcludeTests = e.checked)}
				/>
				<label class="graph-inline-field">
					<span class="graph-inline-label">External</span>
					<select
						class="text-input text-input--narrow"
						bind:value={filterIncludeExternal}
						aria-label="Include external edges"
					>
						<option value="default">Default</option>
						<option value="on">Include</option>
						<option value="off">Exclude</option>
					</select>
				</label>
				<Button variant="primary" size="xs" onclick={applyFilters}>
					Apply
				</Button>
				{#if filterDirty}
					<span class="apply-hint">
						Filter changes are pending — click Apply to reload the relation
						views.
					</span>
				{/if}
			</div>

			<Tabs
				variant="segmented"
				label="Entity views"
				active={currentTab}
				tabs={[
					{ id: 'details', label: 'Details' },
					{ id: 'call-graph', label: 'Call Graph' },
					{ id: 'inheritance', label: 'Inheritance' },
					{ id: 'call-chain', label: 'Call Chain' },
					{ id: 'call-path', label: 'Call Path' },
				]}
				onSelect={(id) => {
					currentTab = id;
					if (id === 'call-graph') loadCallChain('down');
					if (id === 'call-chain') loadCallChain('down');
				}}
			/>

			<!-- Tab Content -->
			{#if currentTab === 'details'}
				<LazyPanel
					loader={() => import('$lib/components/entities/EntityDetail.svelte')}
					componentProps={{
						func: entityStore.currentEntity,
						calls: entityStore.calls?.callees ?? [],
						callers: entityStore.callers?.callers ?? [],
						onNavigate: handleNavigate,
					}}
					loadingMessage="Loading entity details..."
				/>
			{:else if currentTab === 'call-graph'}
				<Card title="Call Graph" subtitle="Visual relationship map">
					<div class="graph-actions">
						<Button
							variant="secondary"
							size="xs"
							onclick={() => openInGraph(String(entityId))}
						>
							Open in Graph Explorer
						</Button>
					</div>
					<LazyPanel
						loader={() => import('$lib/components/entities/CallGraph.svelte')}
						componentProps={{
							nodes: entityStore.callChain,
							focusId: String(entityId),
							onNavigate: handleNavigate,
						}}
						loadingMessage="Loading graph..."
					/>
				</Card>
			{:else if currentTab === 'inheritance'}
				<Card title="Inheritance Tree" subtitle="Class hierarchy">
					<LazyPanel
						loader={() =>
							import('$lib/components/entities/InheritanceTree.svelte')}
						componentProps={{
							inheritance: entityStore.inheritance,
							implementations: entityStore.implementations,
							focusId: String(entityId),
							fallbackName: entityStore.currentEntity?.name ?? null,
							onNavigate: handleNavigate,
						}}
						loadingMessage="Loading tree..."
					/>
				</Card>
			{:else if currentTab === 'call-chain'}
				<Card title="Call Chain" subtitle="Linear execution path">
					{#if entityStore.callChain.length > 0}
						<CallChainList nodes={entityStore.callChain} />
					{:else}
						<p class="empty-hint">No call chain data available</p>
					{/if}
				</Card>
			{:else if currentTab === 'call-path'}
				<Card
					title="Two-point Call Path"
					subtitle="Shortest call path between two functions"
				>
					<div class="path-form">
						<input
							class="text-input text-input--inline"
							type="text"
							placeholder="From function id…"
							bind:value={pathFrom}
							aria-label="Path start function id"
						/>
						<span class="path-arrow" aria-hidden="true"
							><Icon name="arrow-right" /></span
						>
						<input
							class="text-input text-input--inline"
							type="text"
							placeholder="To function id…"
							bind:value={pathTo}
							aria-label="Path end function id"
						/>
						<input
							class="text-input text-input--narrow"
							type="number"
							min="1"
							max="100"
							bind:value={pathDepth}
							aria-label="Path maximum search depth"
						/>
						<Button
							variant="primary"
							size="xs"
							onclick={runCallPath}
							disabled={entityStore.isLoading}
						>
							{entityStore.isLoading ? 'Searching...' : 'Find Path'}
						</Button>
					</div>

					{#if entityStore.callPathError}
						<InlineError message={entityStore.callPathError} />
					{/if}

					{#if entityStore.callPath.length > 0}
						<CallChainList nodes={entityStore.callPath} />
						<p class="path-meta">{entityStore.callPathLength} hop(s)</p>
					{:else if !entityStore.callPathFound && !entityStore.isLoading}
						<p class="empty-hint">
							No call path found between the two functions.
						</p>
					{/if}
				</Card>
			{/if}
		{:else}
			<EmptyState
				message="Entity Not Found"
				hint={`No entity found with ID: ${entityId}`}
			/>
		{/if}
	</div>
</div>

<style>
	.apply-hint {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
	}

	.relation-filter {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: end;
		margin-bottom: 1.5rem;
		padding: 0.75rem;
		border: var(--border-muted);
		background: var(--gray-50);
	}

	.graph-actions {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 1rem;
	}

	.path-form {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}

	.path-arrow {
		color: var(--gray-500);
	}

	.path-meta {
		margin: 0.75rem 0 0;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--gray-500);
	}
</style>
