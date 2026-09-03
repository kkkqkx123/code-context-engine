<script lang="ts">
	import { searchState, searchActions } from '$lib/stores/search';
	import type { QueryType } from '$lib/api/search';

	let showFilters = $state(false);

	function toggleFilters() {
		showFilters = !showFilters;
	}

	function handleExtensionChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const ext = target.value;
		if (target.checked) {
			searchActions.updateFilter('file_extensions', [...$searchState.filters.file_extensions, ext]);
		} else {
			searchActions.updateFilter(
				'file_extensions',
				$searchState.filters.file_extensions.filter(e => e !== ext)
			);
		}
	}

	function handleLanguageChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const lang = target.value;
		if (target.checked) {
			searchActions.updateFilter('languages', [...$searchState.filters.languages, lang]);
		} else {
			searchActions.updateFilter(
				'languages',
				$searchState.filters.languages.filter(l => l !== lang)
			);
		}
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
				<span class="section-label" id="file-extensions-label">File Extensions</span>
				<div class="checkbox-grid" role="group" aria-labelledby="file-extensions-label">
					{#each ['.ts', '.js', '.rs', '.py', '.java', '.go', '.cpp', '.cs'] as ext}
						<label class="checkbox-label">
							<input
								type="checkbox"
								value={ext}
								checked={$searchState.filters.file_extensions.includes(ext)}
								onchange={handleExtensionChange}
							/>
							{ext}
						</label>
					{/each}
				</div>
			</div>

			<div class="filter-section">
				<span class="section-label" id="languages-label">Languages</span>
				<div class="checkbox-grid" role="group" aria-labelledby="languages-label">
					{#each ['typescript', 'javascript', 'rust', 'python', 'java', 'go', 'c++', 'c#'] as lang}
						<label class="checkbox-label">
							<input
								type="checkbox"
								value={lang}
								checked={$searchState.filters.languages.includes(lang)}
								onchange={handleLanguageChange}
							/>
							{lang}
						</label>
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
		grid-template-columns: repeat(4, 1fr);
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

	.checkbox-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
		gap: 0.5rem;
	}

	.checkbox-label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.85rem;
		cursor: pointer;
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
