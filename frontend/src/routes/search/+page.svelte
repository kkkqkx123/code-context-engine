<script lang="ts">
	import { onMount } from 'svelte';
	import { searchState, searchActions } from '$lib/stores/search';
	import SearchInput from '$lib/components/search/SearchInput.svelte';
	import ResultCard from '$lib/components/search/ResultCard.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	// Lazy load FilterPanel component
	let FilterPanel: any = $state(null);
	let filterPanelLoaded = $state(false);
	let filterPanelVisible = $state(false);

	onMount(async () => {
		// Load FilterPanel after initial render
		const module = await import('$lib/components/search/FilterPanel.svelte');
		FilterPanel = module.default;
		filterPanelLoaded = true;
	});

	function handleSearch() {
		searchActions.executeSearch();
	}

	function toggleFilterPanel() {
		filterPanelVisible = !filterPanelVisible;
	}

	function handleNavigate(entityId: string) {
		window.location.href = `/entities/${entityId}`;
	}

	function prevPage() {
		if ($searchState.pagination.page > 1) {
			const newPage = $searchState.pagination.page - 1;
			searchActions.setPage(newPage);
		}
	}

	function nextPage() {
		const totalPages = Math.ceil($searchState.results.length / $searchState.pagination.limit);
		if ($searchState.pagination.page < totalPages) {
			const newPage = $searchState.pagination.page + 1;
			searchActions.setPage(newPage);
		}
	}

	// Get paginated results for display
	let paginatedResults = $derived(searchActions.getPaginatedResults());
	let displayedTotal = $derived(paginatedResults.length);
</script>

<svelte:head>
	<title>Search - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Code Search</h1>
		<p class="page-description">Semantic and keyword-based code search across indexed projects</p>

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

		{#if $searchState.isSearching}
			<div class="loading-indicator">
				<p>Searching...</p>
			</div>
		{:else if $searchState.results.length > 0}
			<div class="results-header">
				<h2>Results ({ $searchState.results.length } total, showing { displayedTotal })</h2>
				<div class="sort-controls">
					<label class="sort-label" for="sort-select">Sort by:</label>
					<select id="sort-select">
						<option value="relevance">Relevance</option>
						<option value="file_path">File Path</option>
						<option value="entity_type">Entity Type</option>
					</select>
				</div>
			</div>

			<div class="results-list">
				{#each paginatedResults as result (result.entity_ids.join(','))}
					<ResultCard {result} onNavigate={handleNavigate} />
				{/each}
			</div>

			{#if $searchState.results.length > $searchState.pagination.limit}
				<div class="pagination">
					<Button
						variant="secondary"
						onclick={prevPage}
						disabled={$searchState.pagination.page === 1}
					>
						Previous
					</Button>
					<span class="page-info">
						Page {$searchState.pagination.page} of {Math.ceil($searchState.results.length / $searchState.pagination.limit)}
					</span>
					<Button
						variant="secondary"
						onclick={nextPage}
						disabled={$searchState.pagination.page >= Math.ceil($searchState.results.length / $searchState.pagination.limit)}
					>
						Next
					</Button>
				</div>
			{/if}
		{:else if $searchState.query}
			<div class="no-results">
				<p>No results found for "{$searchState.query}"</p>
				<p class="hint">Try adjusting your filters or search terms</p>
			</div>
		{/if}
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 3rem;
	}

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

	.results-header {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 2rem;
		align-items: center;
		margin-bottom: 2rem;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--gray-200);
	}

	.results-header h2 {
		font-size: 1.25rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.sort-controls {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.sort-label {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	select {
		padding: 0.5rem;
		border: 1px solid var(--gray-200);
		font-family: 'Space Grotesk', sans-serif;
		outline: none;
	}

	select:focus {
		border-color: var(--accent);
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
