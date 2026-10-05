<script lang="ts">
	import {
		searchState,
		searchActions,
		QUERY_TYPES,
		AGGREGATED_TYPE,
	} from '$lib/stores/search';

	function handleDirectoryChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('directory_prefix', target.value);
	}

	function handleMinScoreChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('min_score', parseFloat(target.value));
	}

	function handleExcludeTypesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('exclude_content_types', target.value);
	}

	function handleExcludeCategoriesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('exclude_categories', target.value);
	}

	function handleIncludeCategoriesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('include_categories', target.value);
	}

	function handleExcludePatternsChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('exclude_patterns', target.value);
	}

	function handleIncludePatternsChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('include_patterns', target.value);
	}

	const RERANK_CHOICES = [
		{ value: 'default', label: 'Server default' },
		{ value: 'on', label: 'On' },
		{ value: 'off', label: 'Off' },
	] as const;

	function rerankValue(): string {
		if ($searchState.filters.enable_rerank === true) return 'on';
		if ($searchState.filters.enable_rerank === false) return 'off';
		return 'default';
	}

	function handleRerankChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		const value =
			target.value === 'on' ? true : target.value === 'off' ? false : null;
		searchActions.updateFilter('enable_rerank', value);
	}

	function handleRerankMaxChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const n = Number(target.value);
		searchActions.updateFilter(
			'rerank_max_candidates',
			Number.isInteger(n) && n > 0 ? n : null,
		);
	}
</script>

<div class="filter-panel">
	<div class="filter-section">
		<span class="section-label" id="query-mode-label">Query Mode</span>
		<div class="mode-tabs" role="group" aria-labelledby="query-mode-label">
			{#each [...QUERY_TYPES, AGGREGATED_TYPE] as mode (mode)}
				<button
					class="tab"
					class:active={$searchState.mode === mode}
					onclick={() => searchActions.setMode(mode)}
				>
					{mode.toUpperCase()}
				</button>
			{/each}
		</div>
	</div>

	{#if $searchState.mode === AGGREGATED_TYPE}
		<div class="filter-section agg-section">
			<span class="section-label">Aggregated Sub-queries (optional)</span>
			<div class="agg-grid">
				<label>
					BM25 Keywords
					<input
						type="text"
						placeholder="e.g. authenticate login verify"
						value={$searchState.bm25Query}
						oninput={(e) =>
							searchActions.setAggSubQuery(
								'bm25',
								(e.target as HTMLInputElement).value,
							)}
					/>
				</label>
				<label>
					Vector Semantics
					<input
						type="text"
						placeholder="e.g. user authentication flow"
						value={$searchState.vectorQuery}
						oninput={(e) =>
							searchActions.setAggSubQuery(
								'vector',
								(e.target as HTMLInputElement).value,
							)}
					/>
				</label>
			</div>
			<p class="hint">
				Leave both empty to auto-run BM25 + Vector on the main query. Filters
				below apply to aggregated search as well.
			</p>
		</div>
	{/if}

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
		<label class="section-label" for="min-score-range"
			>Min Score Threshold</label
		>
		<div class="range-row">
			<input
				id="min-score-range"
				type="range"
				min="0"
				max="1"
				step="0.1"
				value={$searchState.filters.min_score}
				oninput={handleMinScoreChange}
			/>
			<span class="range-value"
				>{$searchState.filters.min_score.toFixed(1)}</span
			>
		</div>
	</div>

	<div class="filter-section">
		<label class="section-label" for="exclude-types-input"
			>Exclude Content Types (comma-separated)</label
		>
		<input
			id="exclude-types-input"
			type="text"
			placeholder="e.g., test, generated, vendor"
			value={$searchState.filters.exclude_content_types}
			oninput={handleExcludeTypesChange}
		/>
	</div>

	<div class="filter-grid">
		<div class="filter-section">
			<label class="section-label" for="exclude-cats-input"
				>Exclude Categories</label
			>
			<input
				id="exclude-cats-input"
				type="text"
				placeholder="e.g., test, generated"
				value={$searchState.filters.exclude_categories}
				oninput={handleExcludeCategoriesChange}
			/>
		</div>
		<div class="filter-section">
			<label class="section-label" for="include-cats-input"
				>Include Categories</label
			>
			<input
				id="include-cats-input"
				type="text"
				placeholder="e.g., test, config"
				value={$searchState.filters.include_categories}
				oninput={handleIncludeCategoriesChange}
			/>
		</div>
	</div>

	<div class="filter-section">
		<label class="section-label" for="exclude-glob-input"
			>Exclude Glob Patterns</label
		>
		<input
			id="exclude-glob-input"
			type="text"
			placeholder="e.g., **/*_test.go, vendor/**"
			value={$searchState.filters.exclude_patterns}
			oninput={handleExcludePatternsChange}
		/>
	</div>

	<div class="filter-section">
		<label class="section-label" for="include-glob-input"
			>Include Glob Patterns</label
		>
		<input
			id="include-glob-input"
			type="text"
			placeholder="e.g., src/**/*.rs"
			value={$searchState.filters.include_patterns}
			oninput={handleIncludePatternsChange}
		/>
	</div>

	<div class="filter-grid">
		<div class="filter-section">
			<label class="section-label" for="rerank-select">Rerank</label>
			<select
				id="rerank-select"
				value={rerankValue()}
				onchange={handleRerankChange}
			>
				{#each RERANK_CHOICES as choice (choice.value)}
					<option value={choice.value}>{choice.label}</option>
				{/each}
			</select>
		</div>
		<div class="filter-section">
			<label class="section-label" for="rerank-max-input"
				>Rerank Max Candidates</label
			>
			<input
				id="rerank-max-input"
				type="number"
				min="1"
				placeholder="server default"
				value={$searchState.filters.rerank_max_candidates ?? ''}
				oninput={handleRerankMaxChange}
			/>
		</div>
	</div>
</div>

<style>
	.filter-panel {
		border: 1px solid var(--gray-200);
		padding: 1.5rem;
		margin-bottom: 1.5rem;
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

	.mode-tabs {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 0.5rem;
	}

	.tab {
		padding: 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-200);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.68rem;
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

	.agg-section {
		padding: 1rem;
		background: var(--gray-100);
		border: 1px solid var(--gray-200);
	}

	.agg-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
	}

	.agg-grid label {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.hint {
		font-size: 0.8rem;
		color: var(--gray-600);
		font-style: italic;
	}

	.range-row {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	input[type='range'] {
		flex: 1;
	}

	.range-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
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

	.filter-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
	}

	@media (max-width: 768px) {
		.mode-tabs,
		.agg-grid,
		.filter-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
