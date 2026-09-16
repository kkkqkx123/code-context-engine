<script lang="ts">
	import { searchState, searchActions } from '$lib/stores/search';
	import type { QueryType } from '$lib/api/search';

	let showFilters = $state(false);

	function toggleFilters() {
		showFilters = !showFilters;
	}

	function handleMinScoreChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('min_score', parseFloat(target.value));
	}

	function handleDirectoryChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('directory_prefix', target.value);
	}
</script>

<div class="filter-panel">
	<button class="filter-toggle" onclick={toggleFilters}>
		<span class="label">Filters</span>
		<span class="arrow" class:open={showFilters}>▼</span>
	</button>

	{#if showFilters}
		<div class="filter-content">
			<div class="filter-section">
				<span class="section-label" id="query-type-label">Query Type</span>
				<div class="query-type-tabs" role="group" aria-labelledby="query-type-label">
					{#each ['vector', 'bm25', 'hybrid'] as type}
						<button
							class="tab"
							class:active={$searchState.queryType === type}
							onclick={() => searchActions.setQueryType(type as QueryType)}
						>
							{type.toUpperCase()}
						</button>
					{/each}
				</div>
			</div>

			<div class="filter-section">
				<label class="section-label" for="directory-input">Directory Prefix</label>
				<input
					id="directory-input"
					type="text"
					placeholder="e.g., src/components"
					value={$searchState.filters.directory_prefix}
					oninput={handleDirectoryChange}
				/>
			</div>

			<div class="filter-section">
				<label class="section-label" for="min-score-range">Min Score Threshold</label>
				<input
					id="min-score-range"
					type="range"
					min="0"
					max="1"
					step="0.1"
					value={$searchState.filters.min_score}
					oninput={handleMinScoreChange}
				/>
				<span class="range-value">{$searchState.filters.min_score.toFixed(1)}</span>
			</div>
		</div>
	{/if}
</div>

<style>
	.filter-panel {
		border: 1px solid var(--gray-200);
		margin-bottom: 2rem;
	}

	.filter-toggle {
		width: 100%;
		padding: 0.75rem 1rem;
		background: var(--gray-100);
		border: none;
		cursor: pointer;
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.75rem;
		letter-spacing: 0.1em;
	}

	.arrow {
		transition: transform 0.2s;
	}

	.arrow.open {
		transform: rotate(180deg);
	}

	.filter-content {
		padding: 1.5rem;
		display: grid;
		gap: 1.5rem;
	}

	.filter-section {
		display: grid;
		gap: 0.75rem;
	}

	.section-label {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.query-type-tabs {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.5rem;
	}

	.tab {
		padding: 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-200);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
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

	input[type='text'] {
		padding: 0.5rem;
		border: 1px solid var(--gray-200);
		font-family: 'Space Grotesk', sans-serif;
		outline: none;
	}

	input[type='text']:focus {
		border-color: var(--accent);
	}

	input[type='range'] {
		width: 100%;
	}

	.range-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}
</style>
