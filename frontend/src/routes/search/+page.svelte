<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Toolbar from '$lib/components/ui/Toolbar.svelte';
	import { onMount } from 'svelte';
	import { SvelteURLSearchParams } from 'svelte/reactivity';
	import { searchStore, AGGREGATED_TYPE } from '$lib/stores/search.svelte';
	import SearchInput from '$lib/components/search/SearchInput.svelte';
	import ResultCard from '$lib/components/search/ResultCard.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import SymbolCandidatePanel from '$lib/components/ui/SymbolCandidatePanel.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Pagination from '$lib/components/ui/Pagination.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
	import LazyPanel from '$lib/components/ui/LazyPanel.svelte';
	import SearchRunPanel from '$lib/components/search/SearchRunPanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { splitTerms } from '$lib/utils/code';

	type QueryView = 'semantic' | 'entities' | 'keyword';

	function viewFromUrl(): QueryView {
		if (typeof window === 'undefined') return 'semantic';
		const value = new SvelteURLSearchParams(window.location.search).get('view');
		return value === 'entities' || value === 'keyword' ? value : 'semantic';
	}

	let view = $state<QueryView>(viewFromUrl());
	let filterPanelVisible = $state(false);
	let compact = $state(false);

	onMount(() => {
		// A shared/reloaded URL carries the query in `q`; replay it once so
		// results are visible without retyping (see stores/search.svelte.ts).
		if (view === 'semantic' && searchStore.query.trim()) {
			void searchStore.executeSearch(1);
		}
	});

	function setView(next: QueryView) {
		view = next;
		const url = new URL(window.location.href);
		url.searchParams.set('view', next);
		window.history.replaceState(null, '', url);
		if (
			next === 'semantic' &&
			searchStore.query.trim() &&
			searchStore.results.length === 0 &&
			!searchStore.isSearching
		) {
			void searchStore.executeSearch(1);
		}
	}

	function handleSearch() {
		selectedIndex = -1;
		void searchStore.executeSearch(1);
	}

	function handleNavigate(entityId: string) {
		goto(resolve(`/entities/${entityId}`));
	}

	function prevPage() {
		if (searchStore.pagination.page > 1) {
			searchStore.setPage(searchStore.pagination.page - 1);
		}
	}

	function nextPage() {
		// Cumulative fetch: re-query with limit = (page + 1) * pageSize.
		void searchStore.executeSearch(searchStore.pagination.page + 1);
	}

	function pickCandidate(stableId: string) {
		searchStore.setQuery(stableId);
		void searchStore.executeSearch(1);
	}

	// Get paginated results for display
	let paginatedResults = $derived(searchStore.getPaginatedResults());
	// Backend has no offset: page N needs a cumulative fetch of N*pageSize
	// rows (see executeSearch in stores/search.svelte.ts). Whether another
	// page exists is decided by the reported total, not by the rows already
	// held — otherwise the first page (rows == page*pageSize) always hides
	// the Next button.
	let hasMore = $derived(
		searchStore.total >
			searchStore.pagination.page * searchStore.pagination.pageSize,
	);
	let queryTerms = $derived(splitTerms(searchStore.query));
	let selectedIndex = $state(-1);

	/** IDE-style shortcuts: `/` focus, j/k move, Enter open. */
	function handleGlobalKeydown(event: KeyboardEvent) {
		const target = event.target as HTMLElement | null;
		const typing =
			!!target &&
			(target.tagName === 'INPUT' ||
				target.tagName === 'TEXTAREA' ||
				target.tagName === 'SELECT' ||
				target.isContentEditable);
		if (!typing && event.key === '/') {
			event.preventDefault();
			(
				document.querySelector('.search-hero') as HTMLInputElement | null
			)?.focus();
			return;
		}
		if (typing || event.metaKey || event.ctrlKey || event.altKey) return;
		const last = paginatedResults.length - 1;
		if (last < 0) return;
		if (event.key === 'j') {
			event.preventDefault();
			selectedIndex = Math.min(last, selectedIndex + 1);
		} else if (event.key === 'k') {
			event.preventDefault();
			selectedIndex = Math.max(0, selectedIndex - 1);
		} else if (event.key === 'Enter' && selectedIndex >= 0) {
			event.preventDefault();
			const id = paginatedResults[selectedIndex]?.entity_ids?.[0];
			if (id !== undefined) handleNavigate(String(id));
		}
	}
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<svelte:head>
	<title>Query Workbench - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Query Workbench"
			subtitle="Semantic, entity and keyword search over the current project"
		/>

		<Tabs
			variant="segmented"
			label="Query workbench views"
			active={view}
			tabs={[
				{ id: 'semantic', label: 'Semantic Search' },
				{ id: 'entities', label: 'Entity Search' },
				{ id: 'keyword', label: 'Keyword Search' },
			]}
			onSelect={(id) => setView(id as QueryView)}
		/>

		{#if view === 'semantic'}
			<SearchInput onSearch={handleSearch} />

			<div class="shortcut-hint">
				Shortcuts: <kbd>/</kbd> focus search · <kbd>j</kbd>/<kbd>k</kbd> select
				result · <kbd>Enter</kbd> open entity
			</div>

			<div class="filter-toggle">
				<Button
					variant="secondary"
					size="xs"
					active={filterPanelVisible}
					onclick={() => (filterPanelVisible = !filterPanelVisible)}
				>
					{filterPanelVisible ? 'Hide Filters' : 'Show Filters'}
				</Button>
				<Button
					variant="secondary"
					size="xs"
					active={compact}
					onclick={() => (compact = !compact)}
				>
					{compact ? 'Comfortable' : 'Compact'}
				</Button>
			</div>

			{#if filterPanelVisible}
				<LazyPanel
					loader={() => import('$lib/components/search/FilterPanel.svelte')}
					loadingMessage="Loading filters..."
				/>
			{/if}

			<SearchRunPanel />

			{#if searchStore.error}
				<ErrorBanner
					message={`Search failed: ${searchStore.error.message}`}
					code={searchStore.error.code}
				/>
				{#if searchStore.error.candidates.length > 0}
					<SymbolCandidatePanel
						candidates={searchStore.error.candidates}
						onPick={(candidate) => pickCandidate(candidate.stable_id)}
					/>
				{/if}
			{/if}

			{#if searchStore.isSearching}
				<Skeleton lines={4} label="Searching code index" />
			{:else if searchStore.results.length > 0}
				<Toolbar>
					<h2 class="results-title">
						Results ({searchStore.total} total)
						{#if searchStore.elapsedMs !== null}
							<span class="elapsed">· {searchStore.elapsedMs}ms</span>
						{/if}
						{#if searchStore.mode === AGGREGATED_TYPE && searchStore.sourcesUsed.length > 0}
							<span class="elapsed"
								>· sources: {searchStore.sourcesUsed.join(', ')}</span
							>
						{/if}
					</h2>
				</Toolbar>

				{#if searchStore.failedSubQueries.length > 0}
					<Notice variant="warning">
						Sub-queries failed (partial results): {searchStore.failedSubQueries.join(
							', ',
						)}
					</Notice>
				{/if}

				{#if searchStore.stale}
					<Notice variant="warning">
						Filters changed — updated results are loading (filters apply
						immediately).
					</Notice>
				{/if}

				{#if searchStore.relationStale}
					<Notice variant="warning">
						Relation snapshot is stale
						{#if searchStore.relationEpoch !== null}
							<span>(epoch {searchStore.relationEpoch})</span>
						{/if}
						— results may be outdated. Re-run the search after indexing catches up.
					</Notice>
				{:else if searchStore.relationEpoch !== null}
					<div class="relation-meta">
						Relation epoch: {searchStore.relationEpoch}
					</div>
				{/if}

				<div class="results-list" class:compact>
					{#each paginatedResults as result, i ((result.entity_ids ?? []).join(',') + '-' + i)}
						<ResultCard
							{result}
							onNavigate={handleNavigate}
							selected={i === selectedIndex}
							{queryTerms}
						/>
					{/each}
				</div>

				{#if hasMore || searchStore.pagination.page > 1}
					<Pagination
						page={searchStore.pagination.page}
						total={null}
						nextLabel="Load more"
						onPrev={prevPage}
						onNext={nextPage}
						prevDisabled={searchStore.pagination.page === 1}
						nextDisabled={!hasMore}
						hint="Results are cumulative: Load more expands the query range and keeps already loaded pages; it does not jump to a precomputed page."
					/>
				{/if}
			{:else if searchStore.query && !searchStore.error}
				<EmptyState
					message={`No results found for "${searchStore.query}"`}
					hint="Try adjusting your filters or search terms"
				/>
			{/if}
		{:else if view === 'entities'}
			<LazyPanel
				loader={() =>
					import('$lib/components/entities/EntitySearchView.svelte')}
				loadingMessage="Loading entity search..."
			/>
		{:else}
			<LazyPanel
				loader={() => import('$lib/components/tools/KeywordSearchTool.svelte')}
				loadingMessage="Loading keyword search..."
			/>
		{/if}
	</div>
</div>

<style>
	.shortcut-hint {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: -0.75rem 0 1.5rem;
	}

	.filter-toggle {
		margin-bottom: 1.5rem;
	}

	.results-title {
		font-size: 1.25rem;
		font-weight: 700;
		letter-spacing: var(--tracking-tight);
	}

	.elapsed {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--gray-500);
	}

	.results-list {
		display: grid;
		gap: 1rem;
		margin-bottom: 2rem;
	}

	.results-list.compact {
		gap: 0.5rem;
	}

	.results-list.compact :global(.result-card) {
		padding: 0.75rem;
	}

	.results-list.compact :global(.code-preview) {
		margin-top: 0.5rem;
	}
</style>
