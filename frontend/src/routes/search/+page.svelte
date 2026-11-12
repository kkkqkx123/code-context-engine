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
	import Notice from '$lib/components/ui/Notice.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import SearchRunPanel from '$lib/components/search/SearchRunPanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { splitTerms } from '$lib/utils/code';

	// Lazy load FilterPanel component
	let FilterPanel: Component | null = $state(null);
	let filterPanelLoaded = $state(false);
	let filterPanelVisible = $state(false);
	let compact = $state(false);

	onMount(async () => {
		// A shared/reloaded URL carries the query in `q`; replay it once so
		// results are visible without retyping (see stores/search.svelte.ts).
		if (searchStore.query.trim()) void searchStore.executeSearch(1);
		// Load FilterPanel after initial render
		const module = await import('$lib/components/search/FilterPanel.svelte');
		FilterPanel = module.default;
		filterPanelLoaded = true;
	});

	function handleSearch() {
		selectedIndex = -1;
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
		// Cumulative fetch: re-query with limit = (page + 1) * pageSize.
		void searchStore.executeSearch(searchStore.pagination.page + 1);
	}

	// Get paginated results for display
	let paginatedResults = $derived(searchStore.getPaginatedResults());
	let totalPages = $derived(searchStore.totalPages());
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
	<title>Search - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Code Search"
			subtitle="Semantic and keyword-based code search across indexed projects"
		/>

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
				onclick={toggleFilterPanel}
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
			{#if filterPanelLoaded && FilterPanel}
				<FilterPanel />
			{:else}
				<LoadingState message="Loading filters..." />
			{/if}
		{/if}

		<SearchRunPanel />

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
				<Notice variant="warning">
					Sub-queries failed (partial results): {searchStore.failedSubQueries.join(
						', ',
					)}
				</Notice>
			{/if}

			{#if searchStore.stale}
				<Notice variant="warning">
					Filters or query mode changed — results may be outdated. Re-run the
					search to apply.
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
	.shortcut-hint {
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		color: var(--gray-500);
		margin: -0.75rem 0 1.5rem;
	}

	kbd {
		border: 1px solid var(--gray-300);
		padding: 0 0.3rem;
		background: var(--gray-100);
	}

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
