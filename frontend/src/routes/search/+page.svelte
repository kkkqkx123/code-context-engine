<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { Component } from 'svelte';
	import Toolbar from '$lib/components/ui/Toolbar.svelte';
	import { onMount } from 'svelte';
	import { searchStore, AGGREGATED_TYPE } from '$lib/stores/search.svelte';
	import SearchInput from '$lib/components/search/SearchInput.svelte';
	import ResultCard from '$lib/components/search/ResultCard.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

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
		void searchStore.executeSearch(1);
	}

	function toggleFilterPanel() {
		filterPanelVisible = !filterPanelVisible;
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
		searchStore.setPage(searchStore.pagination.page + 1);
	}

	// Get paginated results for display
	let paginatedResults = $derived(searchStore.getPaginatedResults());
	let totalPages = $derived(searchStore.totalPages());
	// Backend returns at most `page * pageSize` rows (top-K, no offset), so a
	// next page exists only when we already hold that many rows. `total` is the
	// truncated count; once we've reached it there is nothing more to fetch.
	let hasMore = $derived(
		searchStore.results.length >
			searchStore.pagination.page * searchStore.pagination.pageSize &&
			searchStore.total >
				searchStore.pagination.page * searchStore.pagination.pageSize,
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
			<Button
				variant="secondary"
				size="xs"
				active={filterPanelVisible}
				onclick={toggleFilterPanel}
			>
				{filterPanelVisible ? 'Hide Filters' : 'Show Filters'}
			</Button>
		</div>

		{#if filterPanelVisible}
			{#if filterPanelLoaded && FilterPanel}
				<FilterPanel />
			{:else}
				<LoadingState message="Loading filters..." />
			{/if}
		{/if}

		{#if searchStore.error}
			<ErrorBanner message={`Search failed: ${searchStore.error}`} />
		{/if}

		{#if searchStore.isSearching}
			<LoadingState message="Searching..." />
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
				<div class="warn-banner">
					Sub-queries failed (partial results): {searchStore.failedSubQueries.join(
						', ',
					)}
				</div>
			{/if}

			{#if searchStore.stale}
				<div class="stale-banner">
					Filters or query mode changed — results may be outdated. Re-run the
					search to apply.
				</div>
			{/if}

			{#if searchStore.relationStale}
				<div class="warn-banner">
					Relation snapshot is stale
					{#if searchStore.relationEpoch !== null}
						<span>(epoch {searchStore.relationEpoch})</span>
					{/if}
					— results may be outdated. Re-run the search after indexing catches up.
				</div>
			{:else if searchStore.relationEpoch !== null}
				<div class="relation-meta">
					Relation epoch: {searchStore.relationEpoch}
				</div>
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
						disabled={searchStore.pagination.page === 1}
					>
						Previous
					</Button>
					<span class="page-info">
						Page {searchStore.pagination.page} of {hasMore ? '?' : totalPages}
					</span>
					<Button variant="secondary" onclick={nextPage} disabled={!hasMore}>
						Next
					</Button>
				</div>
			{/if}
		{:else if searchStore.query && !searchStore.error}
			<EmptyState
				message={`No results found for "${searchStore.query}"`}
				hint="Try adjusting your filters or search terms"
			/>
		{/if}
	</div>
</div>

<style>
	.filter-toggle {
		margin-bottom: 1.5rem;
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
</style>
