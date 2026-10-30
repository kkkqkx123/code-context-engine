<script lang="ts">
	/**
	 * Unified health status card.
	 *
	 * Renders the aggregated health summary across external services
	 * (Qdrant, embedding, BM25) from the shared health store.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import { healthStore } from '$lib/stores/health.svelte';
	import { serviceBadge } from '$lib/utils/health-badge';
</script>

<Card title="Unified Status" subtitle="Aggregated health across all external services">
	{#if healthStore.unified}
		<div class="service-grid">
			<div class="service-item">
				<div class="service-label">Overall</div>
				<Badge
					label={serviceBadge(healthStore.unified.healthy).label}
					variant={serviceBadge(healthStore.unified.healthy).variant}
				/>
			</div>
			<div class="service-item">
				<div class="service-label">Qdrant</div>
				<Badge
					label={serviceBadge(healthStore.unified.qdrant?.reachable).label}
					variant={serviceBadge(healthStore.unified.qdrant?.reachable).variant}
				/>
				{#if healthStore.unified.qdrant?.message}
					<div class="service-detail">
						{healthStore.unified.qdrant.message}
					</div>
				{/if}
			</div>
			<div class="service-item">
				<div class="service-label">Embedding</div>
				<Badge
					label={serviceBadge(healthStore.unified.embedding?.reachable).label}
					variant={serviceBadge(healthStore.unified.embedding?.reachable)
						.variant}
				/>
				{#if healthStore.unified.embedding?.message}
					<div class="service-detail">
						{healthStore.unified.embedding.message}
					</div>
				{/if}
			</div>
			<div class="service-item">
				<div class="service-label">BM25</div>
				<Badge
					label={serviceBadge(healthStore.unified.bm25?.reachable).label}
					variant={serviceBadge(healthStore.unified.bm25?.reachable).variant}
				/>
				{#if healthStore.unified.bm25?.message}
					<div class="service-detail">
						{healthStore.unified.bm25.message}
					</div>
				{/if}
			</div>
		</div>
		{#if healthStore.lastUpdated}
			<div class="updated-row">
				<span class="updated-label">Last updated</span>
				<span class="updated-value"
					>{healthStore.lastUpdated.toLocaleTimeString()}</span
				>
			</div>
		{/if}
	{:else}
		<LoadingState
			message={healthStore.isLoading
				? 'Loading health status...'
				: 'No health data yet'}
		/>
	{/if}
</Card>

<style>
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

	@media (max-width: 1024px) {
		.service-grid {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	@media (max-width: 768px) {
		.service-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
