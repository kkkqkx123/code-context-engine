<script lang="ts">
	import {
		searchState,
		searchActions,
		QUERY_TYPES,
		SUB_QUERY_TYPES,
		AGGREGATED_TYPE,
		MAX_SUB_QUERIES,
		type SubQueryDraft,
	} from '$lib/stores/search';

	function addSubQuery() {
		if ($searchState.subQueries.length >= MAX_SUB_QUERIES) return;
		searchActions.setAggSubQueries([
			...$searchState.subQueries,
			{ text: '', query_type: 'bm25', weight: 1.0 },
		]);
	}

	function updateSubQuery(index: number, patch: Partial<SubQueryDraft>) {
		searchActions.setAggSubQueries(
			$searchState.subQueries.map((sq, i) =>
				i === index ? { ...sq, ...patch } : sq,
			),
		);
	}

	function removeSubQuery(index: number) {
		searchActions.setAggSubQueries(
			$searchState.subQueries.filter((_, i) => i !== index),
		);
	}

	function handleDirectoryChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('directory_prefix', target.value);
	}

	function handleMinScoreRangeChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchActions.updateFilter('min_score', parseFloat(target.value));
	}

	function handleMinScoreInputChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const n = parseFloat(target.value);
		if (Number.isNaN(n)) return;
		searchActions.updateFilter('min_score', Math.min(1, Math.max(0, n)));
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
			{#each $searchState.subQueries as sq, i (i)}
				<div class="sub-query-row">
					<input
						class="sub-query-text"
						type="text"
						placeholder="Sub-query text"
						value={sq.text}
						oninput={(e) =>
							updateSubQuery(i, {
								text: (e.target as HTMLInputElement).value,
							})}
					/>
					<select
						value={sq.query_type}
						onchange={(e) =>
							updateSubQuery(i, {
								query_type: (e.target as HTMLSelectElement)
									.value as SubQueryDraft['query_type'],
							})}
					>
						{#each SUB_QUERY_TYPES as qt (qt)}
							<option value={qt}>{qt.toUpperCase()}</option>
						{/each}
					</select>
					<input
						class="sub-query-weight"
						type="number"
						min="0"
						step="0.1"
						title="Weight"
						value={sq.weight}
						oninput={(e) => {
							const n = parseFloat((e.target as HTMLInputElement).value);
							if (!Number.isNaN(n) && n >= 0) updateSubQuery(i, { weight: n });
						}}
					/>
					<button
						class="sub-query-remove"
						type="button"
						aria-label="Remove sub-query"
						onclick={() => removeSubQuery(i)}
					>
						×
					</button>
				</div>
			{/each}
			<button
				class="sub-query-add"
				type="button"
				disabled={$searchState.subQueries.length >= MAX_SUB_QUERIES}
				onclick={addSubQuery}
			>
				+ Add sub-query (max {MAX_SUB_QUERIES})
			</button>
			<p class="hint">
				Leave all empty to auto-run BM25 + Vector on the main query. Each
				sub-query may use its own text, type, and weight; filters below apply
				to aggregated search as well.
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
				step="0.01"
				value={$searchState.filters.min_score}
				oninput={handleMinScoreRangeChange}
			/>
			<input
				id="min-score-input"
				class="range-value-input"
				type="number"
				min="0"
				max="1"
				step="0.01"
				value={$searchState.filters.min_score}
				oninput={handleMinScoreInputChange}
			/>
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

	.sub-query-row {
		display: grid;
		grid-template-columns: 1fr auto 5rem auto;
		gap: 0.5rem;
		align-items: center;
	}

	.sub-query-row select,
	.sub-query-remove {
		padding: 0.4rem 0.5rem;
		border: 1px solid var(--gray-200);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		cursor: pointer;
	}

	.sub-query-remove {
		color: var(--gray-600);
	}

	.sub-query-remove:hover {
		border-color: var(--danger);
		color: var(--danger);
	}

	.sub-query-weight {
		padding: 0.4rem 0.5rem;
		border: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.sub-query-add {
		justify-self: start;
		padding: 0.4rem 0.75rem;
		border: 1px dashed var(--gray-400);
		background: none;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		cursor: pointer;
		color: var(--gray-600);
	}

	.sub-query-add:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.sub-query-add:not(:disabled):hover {
		border-color: var(--accent);
		color: var(--black);
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

	.range-value-input {
		width: 5.5rem;
		padding: 0.35rem 0.5rem;
		border: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
		outline: none;
	}

	.range-value-input:focus {
		border-color: var(--accent);
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
		.sub-query-row,
		.filter-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
