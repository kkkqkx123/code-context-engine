<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import IndexDeletionPanel from '$lib/components/index/IndexDeletionPanel.svelte';
	import {
		indexApi,
		type IndexStatsResponse,
	} from '$lib/api/index';
	import { currentProjectId, onProjectChange } from '$lib/stores/project';
	import { get } from 'svelte/store';
	import { onMount } from 'svelte';

	let stats = $state<IndexStatsResponse['statistics'] | null>(null);
	let statsElapsedMs = $state(0);
	let statsLoading = $state(false);
	let statsError = $state('');

	async function loadStats() {
		statsLoading = true;
		statsError = '';
		stats = null;
		try {
			const response = await indexApi.getStats(get(currentProjectId));
			stats = response.statistics;
			statsElapsedMs = response.elapsed_ms;
		} catch (e) {
			statsError = errorMessage(e) ?? 'Failed to load index statistics';
		} finally {
			statsLoading = false;
		}
	}

	onMount(() => {
		void loadStats();
	});

	// Statistics are project-scoped; never keep the previous project's counts.
	$effect(() => onProjectChange(() => void loadStats()));
</script>

<div class="maintenance">
	<Card title="Index Statistics" subtitle="Counts across all storage backends">
		<div class="stats-actions">
			<Button variant="secondary" onclick={loadStats} disabled={statsLoading}>
				{statsLoading ? 'Loading...' : 'Refresh Stats'}
			</Button>
		</div>
		{#if statsError}
			<InlineError message={statsError} />
		{:else if statsLoading}
			<p class="stats-loading">Loading statistics...</p>
		{:else if stats}
			<div class="stats-grid">
				<div class="stat-item">
					<span class="stat-label">Files</span>
					<span class="stat-value">{stats.total_files.toLocaleString()}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">Entities</span>
					<span class="stat-value">{stats.total_entities.toLocaleString()}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">Relations</span>
					<span class="stat-value">{stats.total_relations.toLocaleString()}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">Vectors</span>
					<span class="stat-value">{stats.total_vectors.toLocaleString()}</span>
				</div>
				<div class="stat-item">
					<span class="stat-label">BM25 Docs</span>
					<span class="stat-value"
						>{stats.total_bm25_documents.toLocaleString()}</span
					>
				</div>
			</div>
			<p class="stats-meta mono">query took {statsElapsedMs} ms</p>
		{:else}
			<p class="placeholder">
				Load the current statistics for the selected project.
			</p>
		{/if}
	</Card>

	<IndexDeletionPanel onDeleted={loadStats} />
</div>

<style>
	.maintenance {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.stats-actions {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 1rem;
	}

	.stats-grid {
		display: grid;
		grid-template-columns: repeat(5, 1fr);
		gap: 1rem;
	}

	.stat-item {
		border: 1px solid var(--gray-200);
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.stat-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.stat-value {
		font-size: 1.4rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.stats-meta {
		margin: 0.75rem 0 0;
	}

	.stats-loading {
		margin-top: 0.75rem;
		color: var(--gray-500);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
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

	@media (max-width: 1024px) {
		.stats-grid {
			grid-template-columns: repeat(2, 1fr);
		}
	}
</style>
