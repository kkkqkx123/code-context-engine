<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { toolsApi, type KeywordSearchResult } from '$lib/api/tools';
	import { currentProjectId } from '$lib/stores/project';
	import { get } from 'svelte/store';

	let query = $state('');
	let termOperator = $state<'or' | 'and'>('or');
	let topN = $state(10);
	let result = $state<KeywordSearchResult | null>(null);
	let loading = $state(false);
	let error = $state('');

	async function handleSearch() {
		const q = query.trim();
		if (!q) return;
		loading = true;
		error = '';
		result = null;
		try {
			result = await toolsApi.keywordSearch({
				project_id: get(currentProjectId),
				query: q,
				term_operator: termOperator,
				top_n: topN
			});
		} catch (e: any) {
			error = e?.message ?? 'Keyword search failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="tool">
	{#if error}
		<div class="tool-error">{error}</div>
	{/if}

	<div class="form-row">
		<label class="field field-wide">
			<span class="field-label">Query</span>
			<input
				class="text-input"
				type="text"
				bind:value={query}
				placeholder="authenticate login verify"
				onkeydown={(e) => e.key === 'Enter' && handleSearch()}
			/>
		</label>
		<label class="field field-narrow">
			<span class="field-label">Term operator</span>
			<select class="text-input" bind:value={termOperator}>
				<option value="or">OR</option>
				<option value="and">AND</option>
			</select>
		</label>
		<label class="field field-narrow">
			<span class="field-label">Top N</span>
			<input class="text-input" type="number" min="1" max="100" bind:value={topN} />
		</label>
		<Button onclick={handleSearch} disabled={!query.trim() || loading}>
			{#if loading}Searching...{:else}Search{/if}
		</Button>
	</div>

	{#if result}
		<div class="result-head">
			<Badge label={`${result.total} matches`} variant="info" />
		</div>
		{#if result.results.length === 0}
			<p class="placeholder">No BM25 hits for the query.</p>
		{:else}
			{#each result.results as item (item.chunk_id)}
				<div class="hit">
					<div class="hit-head">
						<span class="hit-title">{item.title || item.file_path}</span>
						<Badge label={item.score.toFixed(3)} variant="default" />
					</div>
					<p class="hit-location mono">
						{item.file_path}:{item.start_line}-{item.end_line}
					</p>
					<p class="hit-snippet">{item.snippet}</p>
				</div>
			{/each}
		{/if}
	{/if}
</div>

<style>
	.tool {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.tool-error {
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
	}

	.form-row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 0.75rem;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.field-wide {
		flex: 1;
		min-width: 220px;
	}

	.field-narrow {
		width: 120px;
	}

	.field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.text-input {
		padding: 0.5rem 0.6rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
		color: var(--black);
		width: 100%;
	}

	.text-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.result-head {
		display: flex;
		gap: 0.5rem;
	}

	.hit {
		border: 1px solid var(--gray-200);
		padding: 0.6rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.hit-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
	}

	.hit-title {
		font-weight: 700;
		font-size: 0.85rem;
	}

	.hit-location {
		margin: 0;
		word-break: break-all;
	}

	.hit-snippet {
		margin: 0;
		font-size: 0.8rem;
		color: var(--gray-600);
		line-height: 1.5;
	}

	.placeholder {
		color: var(--gray-400);
		font-style: italic;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}
</style>
