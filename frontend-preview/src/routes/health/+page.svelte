<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { onMount } from 'svelte';
	import { healthState, healthActions } from '$lib/stores/health';

	let clearConfirmOpen = $state(false);
	let actionBusy = $state(false);

	onMount(() => {
		// loadAll is refreshed globally by the layout; also fetch per-service
		// diagnostics so the page shows details beyond the unified summary.
		healthActions.loadAll();
		healthActions.loadQdrantHealth();
		healthActions.loadEmbeddingHealth();
		healthActions.loadBm25Health();
	});

	function serviceBadge(healthy: boolean | undefined) {
		if (healthy === undefined) return { label: 'Unknown', variant: 'inactive' as const };
		return healthy
			? { label: 'Healthy', variant: 'success' as const }
			: { label: 'Unhealthy', variant: 'danger' as const };
	}

	async function runAction(action: () => Promise<unknown>) {
		actionBusy = true;
		try {
			await action();
		} finally {
			actionBusy = false;
			clearConfirmOpen = false;
		}
	}
</script>

<svelte:head>
	<title>Health - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader title="Service Health" subtitle="External service reachability and retry queue state">
			{#snippet children()}
				<Button variant="secondary" onclick={() => healthActions.loadAll()} disabled={$healthState.isLoading}>
					Refresh
				</Button>
			{/snippet}
		</PageHeader>

		{#if $healthState.error}
			<div class="error-banner">
				<span>{$healthState.error}</span>
			</div>
		{/if}

		<Card title="Unified Status" subtitle="Aggregated health across all external services">
			{#if $healthState.unified}
				<div class="service-grid">
					<div class="service-item">
						<div class="service-label">Overall</div>
						<Badge
							label={serviceBadge($healthState.unified.healthy).label}
							variant={serviceBadge($healthState.unified.healthy).variant}
						/>
					</div>
					<div class="service-item">
						<div class="service-label">Qdrant</div>
						<Badge
							label={serviceBadge($healthState.unified.qdrant?.reachable).label}
							variant={serviceBadge($healthState.unified.qdrant?.reachable).variant}
						/>
						{#if $healthState.unified.qdrant?.message}
							<div class="service-detail">{$healthState.unified.qdrant.message}</div>
						{/if}
					</div>
					<div class="service-item">
						<div class="service-label">Embedding</div>
						<Badge
							label={serviceBadge($healthState.unified.embedding?.reachable).label}
							variant={serviceBadge($healthState.unified.embedding?.reachable).variant}
						/>
						{#if $healthState.unified.embedding?.message}
							<div class="service-detail">{$healthState.unified.embedding.message}</div>
						{/if}
					</div>
					<div class="service-item">
						<div class="service-label">BM25</div>
						<Badge
							label={serviceBadge($healthState.unified.bm25?.reachable).label}
							variant={serviceBadge($healthState.unified.bm25?.reachable).variant}
						/>
						{#if $healthState.unified.bm25?.message}
							<div class="service-detail">{$healthState.unified.bm25.message}</div>
						{/if}
					</div>
				</div>
				{#if $healthState.lastUpdated}
					<div class="updated-row">
						<span class="updated-label">Last updated</span>
						<span class="updated-value">{$healthState.lastUpdated.toLocaleTimeString()}</span>
					</div>
				{/if}
			{:else}
				<p class="loading-text">{$healthState.isLoading ? 'Loading health status...' : 'No health data yet'}</p>
			{/if}
		</Card>

		<div class="detail-grid">
			<Card title="Qdrant Diagnostics" subtitle="Vector store detailed state">
				{#if $healthState.qdrant}
					<div class="kv-list">
						<div class="kv-row">
							<span class="kv-label">Reachable</span>
							<Badge
								label={serviceBadge($healthState.qdrant.diagnostic?.reachable).label}
								variant={serviceBadge($healthState.qdrant.diagnostic?.reachable).variant}
							/>
						</div>
						{#if $healthState.qdrant.diagnostic?.version}
							<div class="kv-row">
								<span class="kv-label">Version</span>
								<span class="kv-value">{$healthState.qdrant.diagnostic.version}</span>
							</div>
						{/if}
						{#if $healthState.qdrant.diagnostic?.collection_exists !== undefined}
							<div class="kv-row">
								<span class="kv-label">Collection</span>
								<Badge
									label={$healthState.qdrant.diagnostic.collection_exists ? 'Present' : 'Missing'}
									variant={$healthState.qdrant.diagnostic.collection_exists ? 'active' : 'warning'}
								/>
							</div>
						{/if}
						{#if $healthState.qdrant.diagnostic?.points_count !== undefined}
							<div class="kv-row">
								<span class="kv-label">Points</span>
								<span class="kv-value">{$healthState.qdrant.diagnostic.points_count.toLocaleString()}</span>
							</div>
						{/if}
						<div class="kv-row">
							<span class="kv-label">Circuit breaker</span>
							<span class="kv-value">{$healthState.qdrant.circuit_breaker ?? 'unknown'}</span>
						</div>
						{#if $healthState.qdrant.diagnostic?.error}
							<div class="kv-error">{$healthState.qdrant.diagnostic.error}</div>
						{/if}
					</div>
				{:else}
					<p class="loading-text">Diagnostics not loaded</p>
				{/if}
			</Card>

			<Card title="Embedding Service" subtitle="Model serving state">
				{#if $healthState.embedding}
					<div class="kv-list">
						<div class="kv-row">
							<span class="kv-label">Healthy</span>
							<Badge
								label={serviceBadge($healthState.embedding.healthy).label}
								variant={serviceBadge($healthState.embedding.healthy).variant}
							/>
						</div>
						{#if $healthState.embedding.model_name}
							<div class="kv-row">
								<span class="kv-label">Model</span>
								<span class="kv-value">{$healthState.embedding.model_name}</span>
							</div>
						{/if}
						{#if $healthState.embedding.message}
							<div class="kv-row">
								<span class="kv-label">Message</span>
								<span class="kv-value">{$healthState.embedding.message}</span>
							</div>
						{/if}
					</div>
				{:else}
					<p class="loading-text">Embedding health not loaded</p>
				{/if}
			</Card>

			<Card title="BM25 Index" subtitle="Keyword index state">
				{#if $healthState.bm25}
					<div class="kv-list">
						<div class="kv-row">
							<span class="kv-label">Connected</span>
							<Badge
								label={$healthState.bm25.connected ? 'Connected' : 'Disconnected'}
								variant={$healthState.bm25.connected ? 'success' : 'danger'}
							/>
						</div>
						{#if $healthState.bm25.enabled !== undefined}
							<div class="kv-row">
								<span class="kv-label">Enabled</span>
								<Badge
									label={$healthState.bm25.enabled ? 'Yes' : 'No'}
									variant={$healthState.bm25.enabled ? 'active' : 'inactive'}
								/>
							</div>
						{/if}
						{#if $healthState.bm25.index_path}
							<div class="kv-row">
								<span class="kv-label">Index path</span>
								<span class="kv-value mono">{$healthState.bm25.index_path}</span>
							</div>
						{/if}
					</div>
				{:else}
					<p class="loading-text">BM25 health not loaded</p>
				{/if}
			</Card>
		</div>

		<Card title="Retry Queue" subtitle="Pending write-behind operations">
			{#if $healthState.retryQueue}
				<div class="retry-row">
					<div class="retry-stat">
						<span class="kv-label">Pending operations</span>
						<span class="retry-count">{$healthState.retryQueue.pending_count}</span>
					</div>
					<Badge
						label={$healthState.retryQueue.is_empty ? 'Empty' : 'Backlog'}
						variant={$healthState.retryQueue.is_empty ? 'success' : 'warning'}
					/>
				</div>
				<div class="retry-actions">
					<Button
						variant="secondary"
						onclick={() => runAction(() => healthActions.processRetryQueue())}
						disabled={actionBusy || $healthState.retryQueue?.is_empty}
					>
						{actionBusy ? 'Working...' : 'Process Queue'}
					</Button>
					<Button
						variant="danger"
						onclick={() => (clearConfirmOpen = true)}
						disabled={actionBusy || $healthState.retryQueue?.is_empty}
					>
						Clear Queue
					</Button>
				</div>
			{:else}
				<p class="loading-text">Retry queue status not loaded</p>
			{/if}
		</Card>
	</div>
</div>

{#if clearConfirmOpen}
	<div
		class="dialog-overlay"
		onclick={() => (clearConfirmOpen = false)}
		onkeydown={(e) => {
			if (e.key === 'Escape') clearConfirmOpen = false;
		}}
		role="button"
		tabindex="0"
		aria-label="Close dialog"
	>
		<div
			class="dialog"
			onclick={(e) => e.stopPropagation()}
			role="dialog"
			aria-modal="true"
			aria-labelledby="retry-dialog-title"
			tabindex="-1"
			onkeydown={(e) => {
				if (e.key === 'Escape') clearConfirmOpen = false;
			}}
		>
			<h2 id="retry-dialog-title">Clear Retry Queue</h2>
			<p class="dialog-warning">
				All pending retry operations will be discarded. Data they were meant to write will not be retried.
			</p>
			<div class="dialog-actions">
				<Button variant="secondary" onclick={() => (clearConfirmOpen = false)}>Cancel</Button>
				<Button variant="danger" onclick={() => runAction(() => healthActions.clearRetryQueue())} disabled={actionBusy}>
					{actionBusy ? 'Clearing...' : 'Clear Queue'}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.error-banner {
		background: var(--danger);
		color: var(--white);
		padding: 0.75rem 1rem;
		margin-bottom: 1.5rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}

	.service-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 1.5rem;
	}

	.service-item {
		padding: 1rem;
		border: 1px solid var(--gray-200);
	}

	.service-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.service-detail {
		margin-top: 0.5rem;
		font-size: 0.8rem;
		color: var(--gray-600);
		overflow-wrap: anywhere;
	}

	.updated-row {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
		display: flex;
		gap: 0.5rem;
		align-items: baseline;
	}

	.updated-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.updated-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		color: var(--gray-600);
	}

	.detail-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1.5rem;
		margin-bottom: 1.5rem;
	}

	.detail-grid :global(.card) {
		margin-bottom: 0;
	}

	.kv-list {
		display: flex;
		flex-direction: column;
	}

	.kv-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--gray-100);
	}

	.kv-row:last-child {
		border-bottom: none;
	}

	.kv-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.kv-value {
		font-size: 0.85rem;
		color: var(--black);
		text-align: right;
		overflow-wrap: anywhere;
	}

	.kv-value.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.kv-error {
		margin-top: 0.5rem;
		padding: 0.5rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.retry-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}

	.retry-stat {
		display: flex;
		align-items: baseline;
		gap: 1rem;
	}

	.retry-count {
		font-size: 1.75rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.retry-actions {
		display: flex;
		gap: 0.75rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.loading-text {
		color: var(--gray-400);
		font-style: italic;
		padding: 1rem 0;
	}

	.dialog-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.dialog {
		background: var(--white);
		border: 2px solid var(--danger);
		padding: 2rem;
		max-width: 480px;
		width: 90%;
	}

	.dialog h2 {
		margin-bottom: 1rem;
		color: var(--black);
	}

	.dialog-warning {
		color: var(--gray-600);
		margin-bottom: 2rem;
		line-height: 1.6;
	}

	.dialog-actions {
		display: flex;
		gap: 1rem;
		justify-content: flex-end;
	}

	@media (max-width: 1024px) {
		.service-grid {
			grid-template-columns: repeat(2, 1fr);
		}

		.detail-grid {
			grid-template-columns: 1fr;
		}
	}

	@media (max-width: 768px) {
		.service-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
