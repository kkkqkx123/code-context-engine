<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { Component } from 'svelte';
	import Toolbar from '$lib/components/ui/Toolbar.svelte';
	import { onMount } from 'svelte';
	import {
		searchState,
		searchActions,
		AGGREGATED_TYPE,
	} from '$lib/stores/search';
	import SearchInput from '$lib/components/search/SearchInput.svelte';
	import ResultCard from '$lib/components/search/ResultCard.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	// Lazy load FilterPanel component
	let FilterPanel: Component | null = $state(null);
	let filterPanelLoaded = $state(false);
	let filterPanelVisible = $state(false);

	onMount(async () => {
		// Load FilterPanel after initial render
		const module = await import('$lib/components/search/FilterPanel.svelte');
		FilterPanel = module.default;
		filterPanelLoaded = true;
	});

	function handleSearch() {
		void searchActions.executeSearch(1);
	}

	function toggleFilterPanel() {
		filterPanelVisible = !filterPanelVisible;
	}

	function handleNavigate(entityId: string) {
		window.location.href = `/entities/${entityId}`;
	}

	function prevPage() {
		if ($searchState.pagination.page > 1) {
			searchActions.setPage($searchState.pagination.page - 1);
		}
	}

	function nextPage() {
		searchActions.setPage($searchState.pagination.page + 1);
	}

	// Get paginated results for display
	let paginatedResults = $derived(searchActions.getPaginatedResults());
	let totalPages = $derived(searchActions.totalPages());
	// Backend returns at most `page * pageSize` rows (top-K, no offset), so a
	// next page exists only when we already hold that many rows. `total` is the
	// truncated count; once we've reached it there is nothing more to fetch.
	let hasMore = $derived(
		$searchState.results.length >
			$searchState.pagination.page * $searchState.pagination.pageSize &&
			$searchState.total >
				$searchState.pagination.page * $searchState.pagination.pageSize,
	);
</script>

<svelte:head>
	<title>Search - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Code Search"
			subtitle="Semantic and keyword-based code search across indexed projects"
		/>

		<SearchInput onSearch={handleSearch} />

		<div class="filter-toggle">
			<button
				class="filter-toggle-btn"
				class:active={filterPanelVisible}
				onclick={toggleFilterPanel}
			>
				{filterPanelVisible ? 'Hide Filters' : 'Show Filters'}
			</button>
		</div>

		{#if filterPanelVisible}
			{#if filterPanelLoaded && FilterPanel}
				<FilterPanel />
			{:else}
				<div class="loading-filter">Loading filters...</div>
			{/if}
		{/if}

		{#if $searchState.error}
			<div class="error-banner">Search failed: {$searchState.error}</div>
		{/if}

		{#if $searchState.isSearching}
			<div class="loading-indicator">
				<p>Searching...</p>
			</div>
		{:else if $searchState.results.length > 0}
			<Toolbar>
				<h2 class="results-title">
					Results ({$searchState.total} total)
					{#if $searchState.elapsedMs !== null}
						<span class="elapsed">· {$searchState.elapsedMs}ms</span>
					{/if}
					{#if $searchState.mode === AGGREGATED_TYPE && $searchState.sourcesUsed.length > 0}
						<span class="elapsed"
							>· sources: {$searchState.sourcesUsed.join(', ')}</span
						>
					{/if}
				</h2>
			</Toolbar>

			{#if $searchState.failedSubQueries.length > 0}
				<div class="warn-banner">
					Sub-queries failed (partial results): {$searchState.failedSubQueries.join(
						', ',
					)}
				</div>
			{/if}

			{#if $searchState.stale}
				<div class="stale-banner">
					Filters or query mode changed — results may be outdated. Re-run the
					search to apply.
				</div>
			{/if}

			{#if $searchState.relationStale}
				<div class="warn-banner">
					Relation snapshot is stale
					{#if $searchState.relationEpoch !== null}
						<span>(epoch {$searchState.relationEpoch})</span>
					{/if}
					— results may be outdated. Re-run the search after indexing catches
					up.
				</div>
			{:else if $searchState.relationEpoch !== null}
				<div class="relation-meta">Relation epoch: {$searchState.relationEpoch}</div>
			{/if}

			<div class="results-list">
				{#each paginatedResults as result, i ((result.entity_ids ?? []).join(',') + '-' + i)}
					<ResultCard {result} onNavigate={handleNavigate} />
				{/each}
			</div>

			{#if totalPages > 1 || hasMore}
				<div class="pagination">
					<Button
						variant="secondary"
						onclick={prevPage}
						disabled={$searchState.pagination.page === 1}
					>
						Previous
					</Button>
					<span class="page-info">
						Page {$searchState.pagination.page} of {hasMore ? '?' : totalPages}
					</span>
					<Button variant="secondary" onclick={nextPage} disabled={!hasMore}>
						Next
					</Button>
				</div>
			{/if}
		{:else if $searchState.query && !$searchState.error}
			<div class="no-results">
				<p>No results found for "{$searchState.query}"</p>
				<p class="hint">Try adjusting your filters or search terms</p>
			</div>
		{/if}
	</div>
</div>

<style>
	.filter-toggle {
		margin-bottom: 1.5rem;
	}

	.filter-toggle-btn {
		padding: 0.5rem 1rem;
		background: none;
		border: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		transition: all 0.2s ease;
		color: var(--gray-600);
	}

	.filter-toggle-btn:hover {
		border-color: var(--accent);
		color: var(--black);
	}

	.filter-toggle-btn.active {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--white);
	}

	.loading-filter {
		padding: 2rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
		border: 1px dashed var(--gray-200);
		margin-bottom: 1.5rem;
	}

	.loading-indicator {
		padding: 3rem;
		text-align: center;
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}

	.results-title {
		font-size: 1.25rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.elapsed {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--gray-500);
	}

	.error-banner {
		padding: 1rem;
		margin-bottom: 1.5rem;
		background: var(--danger-bg);
		border-left: 4px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.warn-banner {
		padding: 0.75rem 1rem;
		margin-bottom: 1rem;
		background: var(--warning-bg, var(--gray-100));
		border-left: 4px solid var(--warning, var(--gray-400));
		color: var(--gray-700);
		font-size: 0.85rem;
	}

	.stale-banner {
		padding: 0.75rem 1rem;
		margin-bottom: 1rem;
		background: var(--info-bg);
		border-left: 4px solid var(--info);
		color: var(--gray-700);
		font-size: 0.85rem;
	}

	.results-list {
		display: grid;
		gap: 1rem;
		margin-bottom: 2rem;
	}

	.pagination {
		display: flex;
		justify-content: center;
		align-items: center;
		gap: 2rem;
		padding-top: 2rem;
		border-top: 1px solid var(--gray-200);
	}

	.page-info {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.no-results {
		padding: 4rem 2rem;
		text-align: center;
		border: 1px solid var(--gray-200);
	}

	.no-results p {
		margin-bottom: 0.5rem;
	}

	.hint {
		color: var(--gray-400);
		font-style: italic;
	}
</style>
