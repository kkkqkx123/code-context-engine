<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import ToolFormRow from '$lib/components/tools/ToolFormRow.svelte';
	import { toolsApi, type KeywordSearchResult } from '$lib/api/tools';
	import { projectStore } from '$lib/stores/project.svelte';

	let query = $state('');
	let termOperator = $state<'or' | 'and'>('or');
	let topN = $state(10);
	let offset = $state(0);
	let epochText = $state('');
	let result = $state<KeywordSearchResult | null>(null);
	let loading = $state(false);
	let error = $state('');

	async function handleSearch() {
		const q = query.trim();
		if (!q) return;
		loading = true;
		error = '';
		result = null;
		const trimmedEpoch = epochText.trim();
		const epoch = trimmedEpoch === '' ? null : Number(trimmedEpoch);
		if (epoch !== null && (!Number.isInteger(epoch) || epoch < 0)) {
			error =
				'Epoch must be a non-negative integer or empty for the current generation';
			loading = false;
			return;
		}
		try {
			result = await toolsApi.keywordSearch({
				project_id: projectStore.currentProjectId,
				query: q,
				term_operator: termOperator,
				top_n: topN,
				offset,
				epoch,
			});
		} catch (e) {
			error = errorMessage(e) ?? 'Keyword search failed';
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<ToolFormRow>
		<ToolField label="Query" wide>
			<input
				class="text-input"
				type="text"
				bind:value={query}
				placeholder="authenticate login verify"
				onkeydown={(e) => e.key === 'Enter' && handleSearch()}
			/>
		</ToolField>
		<ToolField label="Term operator">
			<select class="text-input" bind:value={termOperator}>
				<option value="or">OR</option>
				<option value="and">AND</option>
			</select>
		</ToolField>
		<ToolField label="Top N">
			<input
				class="text-input"
				type="number"
				min="1"
				max="100"
				bind:value={topN}
			/>
		</ToolField>
		<ToolField label="Offset">
			<input class="text-input" type="number" min="0" bind:value={offset} />
		</ToolField>
		<ToolField label="Epoch">
			<input
				class="text-input"
				type="text"
				inputmode="numeric"
				placeholder="auto"
				bind:value={epochText}
				aria-label="Index epoch for version-aware filtering, empty for current"
			/>
		</ToolField>
		<Button onclick={handleSearch} disabled={!query.trim() || loading}>
			{#if loading}Searching...{:else}Search{/if}
		</Button>
	</ToolFormRow>

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
</ToolShell>

<style>
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
