<script lang="ts">
	import {
		searchStore,
		QUERY_TYPES,
		SUB_QUERY_TYPES,
		AGGREGATED_TYPE,
		MAX_SUB_QUERIES,
		type SubQueryDraft,
	} from '$lib/stores/search.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	function addSubQuery() {
		if (searchStore.subQueries.length >= MAX_SUB_QUERIES) return;
		searchStore.setAggSubQueries([
			...searchStore.subQueries,
			{ text: '', query_type: 'bm25', weight: 1.0 },
		]);
	}

	function updateSubQuery(index: number, patch: Partial<SubQueryDraft>) {
		searchStore.setAggSubQueries(
			searchStore.subQueries.map((sq, i) =>
				i === index ? { ...sq, ...patch } : sq,
			),
		);
	}

	function removeSubQuery(index: number) {
		searchStore.setAggSubQueries(
			searchStore.subQueries.filter((_, i) => i !== index),
		);
	}

	function handleDirectoryChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('directory_prefix', target.value);
	}

	function handleMinScoreRangeChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('min_score', parseFloat(target.value));
	}

	function handleMinScoreInputChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const n = parseFloat(target.value);
		if (Number.isNaN(n)) return;
		searchStore.updateFilter('min_score', Math.min(1, Math.max(0, n)));
	}

	function handleExcludeTypesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('exclude_content_types', target.value);
	}

	function handleExcludeCategoriesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('exclude_categories', target.value);
	}

	function handleIncludeCategoriesChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('include_categories', target.value);
	}

	function handleExcludePatternsChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('exclude_patterns', target.value);
	}

	function handleIncludePatternsChange(event: Event) {
		const target = event.target as HTMLInputElement;
		searchStore.updateFilter('include_patterns', target.value);
	}

	const RERANK_CHOICES = [
		{ value: 'default', label: 'Server default' },
		{ value: 'on', label: 'On' },
		{ value: 'off', label: 'Off' },
	] as const;

	function rerankValue(): string {
		if (searchStore.filters.enable_rerank === true) return 'on';
		if (searchStore.filters.enable_rerank === false) return 'off';
		return 'default';
	}

	function handleRerankChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		const value =
			target.value === 'on' ? true : target.value === 'off' ? false : null;
		searchStore.updateFilter('enable_rerank', value);
	}

	function handleRerankMaxChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const n = Number(target.value);
		searchStore.updateFilter(
			'rerank_max_candidates',
			Number.isInteger(n) && n > 0 ? n : null,
		);
	}

	const INTENT_CHOICES = [
		{ value: '', label: 'Auto-detect' },
		{ value: 'semantic', label: 'Semantic' },
		{ value: 'keyword', label: 'Keyword' },
		{ value: 'hybrid', label: 'Hybrid' },
		{ value: 'entity', label: 'Entity' },
	] as const;

	function handleIntentChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		searchStore.updateFilter('query_intent', target.value);
	}

	const EXPAND_CHOICES = [
		{ value: 'default', label: 'Server default' },
		{ value: 'on', label: 'On' },
		{ value: 'off', label: 'Off' },
	] as const;

	function expandValue(): string {
		if (searchStore.filters.expand === true) return 'on';
		if (searchStore.filters.expand === false) return 'off';
		return 'default';
	}

	function handleExpandChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		const value =
			target.value === 'on' ? true : target.value === 'off' ? false : null;
		searchStore.updateFilter('expand', value);
	}
</script>

<div class="filter-panel">
	<div class="filter-section">
		<span class="section-label" id="query-mode-label">Query Mode</span>
		<div class="mode-tabs" role="group" aria-labelledby="query-mode-label">
			{#each [...QUERY_TYPES, AGGREGATED_TYPE] as mode (mode)}
				<Button
					variant="tab"
					size="xs"
					active={searchStore.mode === mode}
					onclick={() => searchStore.setMode(mode)}
				>
					{mode.toUpperCase()}
				</Button>
			{/each}
		</div>
	</div>

	{#if searchStore.mode === AGGREGATED_TYPE}
		<div class="filter-section agg-section">
			<span class="section-label">Aggregated Sub-queries (optional)</span>
			{#each searchStore.subQueries as sq, i (i)}
				<div class="sub-query-row">
					<input
						class="text-input"
						type="text"
						placeholder="Sub-query text"
						value={sq.text}
						oninput={(e) =>
							updateSubQuery(i, {
								text: (e.target as HTMLInputElement).value,
							})}
					/>
					<select
						class="text-input"
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
						class="text-input"
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
					<Button
						variant="ghost"
						size="xs"
						onclick={() => removeSubQuery(i)}
						aria-label="Remove sub-query"
					>
						<Icon name="close" />
					</Button>
				</div>
			{/each}
			<Button
				variant="secondary"
				size="xs"
				disabled={searchStore.subQueries.length >= MAX_SUB_QUERIES}
				onclick={addSubQuery}
			>
				+ Add sub-query (max {MAX_SUB_QUERIES})
			</Button>
			<p class="hint">
				Leave all empty to auto-run BM25 + Vector on the main query. Each
				sub-query may use its own text, type, and weight; filters below apply to
				aggregated search as well.
			</p>
		</div>
	{/if}

	<div class="filter-section">
		<label class="section-label" for="directory-input">Directory Prefix</label>
		<input
			class="text-input"
			id="directory-input"
			type="text"
			placeholder="e.g., src/components"
			value={searchStore.filters.directory_prefix}
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
				value={searchStore.filters.min_score}
				oninput={handleMinScoreRangeChange}
			/>
			<input
				id="min-score-input"
				class="text-input range-value-input"
				type="number"
				min="0"
				max="1"
				step="0.01"
				value={searchStore.filters.min_score}
				oninput={handleMinScoreInputChange}
			/>
		</div>
	</div>

	<div class="filter-section">
		<label class="section-label" for="exclude-types-input"
			>Exclude Content Types (comma-separated)</label
		>
		<input
			class="text-input"
			id="exclude-types-input"
			type="text"
			placeholder="e.g., test, generated, vendor"
			value={searchStore.filters.exclude_content_types}
			oninput={handleExcludeTypesChange}
		/>
	</div>

	<div class="filter-grid">
		<div class="filter-section">
			<label class="section-label" for="exclude-cats-input"
				>Exclude Categories</label
			>
			<input
				class="text-input"
				id="exclude-cats-input"
				type="text"
				placeholder="e.g., test, generated"
				value={searchStore.filters.exclude_categories}
				oninput={handleExcludeCategoriesChange}
			/>
		</div>
		<div class="filter-section">
			<label class="section-label" for="include-cats-input"
				>Include Categories</label
			>
			<input
				class="text-input"
				id="include-cats-input"
				type="text"
				placeholder="e.g., test, config"
				value={searchStore.filters.include_categories}
				oninput={handleIncludeCategoriesChange}
			/>
		</div>
	</div>

	<div class="filter-section">
		<label class="section-label" for="exclude-glob-input"
			>Exclude Glob Patterns</label
		>
		<input
			class="text-input"
			id="exclude-glob-input"
			type="text"
			placeholder="e.g., **/*_test.go, vendor/**"
			value={searchStore.filters.exclude_patterns}
			oninput={handleExcludePatternsChange}
		/>
	</div>

	<div class="filter-section">
		<label class="section-label" for="include-glob-input"
			>Include Glob Patterns</label
		>
		<input
			class="text-input"
			id="include-glob-input"
			type="text"
			placeholder="e.g., src/**/*.rs"
			value={searchStore.filters.include_patterns}
			oninput={handleIncludePatternsChange}
		/>
	</div>

	<div class="filter-grid">
		<div class="filter-section">
			<label class="section-label" for="rerank-select">Rerank</label>
			<select
				class="text-input"
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
				class="text-input"
				id="rerank-max-input"
				type="number"
				min="1"
				placeholder="server default"
				value={searchStore.filters.rerank_max_candidates ?? ''}
				oninput={handleRerankMaxChange}
			/>
		</div>
	</div>

	<div class="filter-grid">
		<div class="filter-section">
			<label class="section-label" for="intent-select">Query Intent</label>
			<select
				class="text-input"
				id="intent-select"
				value={searchStore.filters.query_intent}
				onchange={handleIntentChange}
			>
				{#each INTENT_CHOICES as choice (choice.value)}
					<option value={choice.value}>{choice.label}</option>
				{/each}
			</select>
		</div>
		<div class="filter-section">
			<label class="section-label" for="expand-select"
				>Call-graph Expansion</label
			>
			<select
				class="text-input"
				id="expand-select"
				value={expandValue()}
				onchange={handleExpandChange}
			>
				{#each EXPAND_CHOICES as choice (choice.value)}
					<option value={choice.value}>{choice.label}</option>
				{/each}
			</select>
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
		border: 1px solid var(--gray-300);
		padding: 0.35rem;
		width: fit-content;
		max-width: 100%;
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
		flex-shrink: 0;
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
