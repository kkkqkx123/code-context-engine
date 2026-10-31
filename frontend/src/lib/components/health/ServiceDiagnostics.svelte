<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import { healthStore } from '$lib/stores/health.svelte';
	import { serviceBadge } from '$lib/utils/health-badge';
</script>

<!--
	Per-service diagnostic cards for the health page: Qdrant, embedding
	service and BM25 index. Each card renders the raw diagnostic payload
	collected by the health store.
-->
<div class="detail-grid">
	<Card title="Qdrant Diagnostics" subtitle="Vector store detailed state">
		{#if healthStore.qdrant}
			<div class="kv-list">
				<div class="kv-row">
					<span class="kv-label">Reachable</span>
					<Badge
						label={serviceBadge(healthStore.qdrant.diagnostic?.reachable).label}
						variant={serviceBadge(healthStore.qdrant.diagnostic?.reachable)
							.variant}
					/>
				</div>
				{#if healthStore.qdrant.diagnostic?.version}
					<div class="kv-row">
						<span class="kv-label">Version</span>
						<span class="kv-value">{healthStore.qdrant.diagnostic.version}</span
						>
					</div>
				{/if}
				{#if healthStore.qdrant.diagnostic?.collection_exists !== undefined}
					<div class="kv-row">
						<span class="kv-label">Collection</span>
						<Badge
							label={healthStore.qdrant.diagnostic.collection_exists
								? 'Present'
								: 'Missing'}
							variant={healthStore.qdrant.diagnostic.collection_exists
								? 'success'
								: 'warning'}
						/>
					</div>
				{/if}
				{#if healthStore.qdrant.diagnostic?.points_count !== undefined}
					<div class="kv-row">
						<span class="kv-label">Points</span>
						<span class="kv-value"
							>{healthStore.qdrant.diagnostic.points_count.toLocaleString()}</span
						>
					</div>
				{/if}
				<div class="kv-row">
					<span class="kv-label">Circuit breaker</span>
					<span class="kv-value"
						>{healthStore.qdrant.circuit_breaker ?? 'unknown'}</span
					>
				</div>
				{#if healthStore.qdrant.diagnostic?.error}
					<InlineError message={healthStore.qdrant.diagnostic.error} />
				{/if}
			</div>
		{:else}
			<LoadingState message="Diagnostics not loaded" />
		{/if}
	</Card>

	<Card title="Embedding Service" subtitle="Model serving state">
		{#if healthStore.embedding}
			<div class="kv-list">
				<div class="kv-row">
					<span class="kv-label">Healthy</span>
					<Badge
						label={serviceBadge(healthStore.embedding.healthy).label}
						variant={serviceBadge(healthStore.embedding.healthy).variant}
					/>
				</div>
				{#if healthStore.embedding.model_name}
					<div class="kv-row">
						<span class="kv-label">Model</span>
						<span class="kv-value">{healthStore.embedding.model_name}</span>
					</div>
				{/if}
				{#if healthStore.embedding.message}
					<div class="kv-row">
						<span class="kv-label">Message</span>
						<span class="kv-value">{healthStore.embedding.message}</span>
					</div>
				{/if}
			</div>
		{:else}
			<LoadingState message="Embedding health not loaded" />
		{/if}
	</Card>

	<Card title="BM25 Index" subtitle="Keyword index state">
		{#if healthStore.bm25}
			<div class="kv-list">
				<div class="kv-row">
					<span class="kv-label">Connected</span>
					<Badge
						label={healthStore.bm25.connected ? 'Connected' : 'Disconnected'}
						variant={healthStore.bm25.connected ? 'success' : 'danger'}
					/>
				</div>
				{#if healthStore.bm25.enabled !== undefined}
					<div class="kv-row">
						<span class="kv-label">Enabled</span>
						<Badge
							label={healthStore.bm25.enabled ? 'Yes' : 'No'}
							variant={healthStore.bm25.enabled ? 'success' : 'inactive'}
						/>
					</div>
				{/if}
				{#if healthStore.bm25.index_path}
					<div class="kv-row">
						<span class="kv-label">Index path</span>
						<span class="kv-value mono">{healthStore.bm25.index_path}</span>
					</div>
				{/if}
			</div>
		{:else}
			<LoadingState message="BM25 health not loaded" />
		{/if}
	</Card>
</div>

<style>
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

	@media (max-width: 1024px) {
		.detail-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
