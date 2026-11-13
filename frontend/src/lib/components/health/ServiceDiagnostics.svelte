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
<div class="card-grid">
	<Card title="Qdrant Diagnostics" subtitle="Vector store detailed state">
		{#if healthStore.qdrant}
			<div class="desc-list">
				<div class="desc-row">
					<span class="desc-term">Reachable</span>
					<Badge
						label={serviceBadge(healthStore.qdrant.diagnostic?.reachable).label}
						variant={serviceBadge(healthStore.qdrant.diagnostic?.reachable)
							.variant}
					/>
				</div>
				{#if healthStore.qdrant.diagnostic?.version}
					<div class="desc-row">
						<span class="desc-term">Version</span>
						<span class="desc-value"
							>{healthStore.qdrant.diagnostic.version}</span
						>
					</div>
				{/if}
				{#if healthStore.qdrant.diagnostic?.collection_exists !== undefined}
					<div class="desc-row">
						<span class="desc-term">Collection</span>
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
					<div class="desc-row">
						<span class="desc-term">Points</span>
						<span class="desc-value"
							>{healthStore.qdrant.diagnostic.points_count.toLocaleString()}</span
						>
					</div>
				{/if}
				<div class="desc-row">
					<span class="desc-term">Circuit breaker</span>
					<span class="desc-value"
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
			<div class="desc-list">
				<div class="desc-row">
					<span class="desc-term">Healthy</span>
					<Badge
						label={serviceBadge(healthStore.embedding.healthy).label}
						variant={serviceBadge(healthStore.embedding.healthy).variant}
					/>
				</div>
				{#if healthStore.embedding.model_name}
					<div class="desc-row">
						<span class="desc-term">Model</span>
						<span class="desc-value">{healthStore.embedding.model_name}</span>
					</div>
				{/if}
				{#if healthStore.embedding.message}
					<div class="desc-row">
						<span class="desc-term">Message</span>
						<span class="desc-value">{healthStore.embedding.message}</span>
					</div>
				{/if}
			</div>
		{:else}
			<LoadingState message="Embedding health not loaded" />
		{/if}
	</Card>

	<Card title="BM25 Index" subtitle="Keyword index state">
		{#if healthStore.bm25}
			<div class="desc-list">
				<div class="desc-row">
					<span class="desc-term">Connected</span>
					<Badge
						label={healthStore.bm25.connected ? 'Connected' : 'Disconnected'}
						variant={healthStore.bm25.connected ? 'success' : 'danger'}
					/>
				</div>
				{#if healthStore.bm25.enabled !== undefined}
					<div class="desc-row">
						<span class="desc-term">Enabled</span>
						<Badge
							label={healthStore.bm25.enabled ? 'Yes' : 'No'}
							variant={healthStore.bm25.enabled ? 'success' : 'inactive'}
						/>
					</div>
				{/if}
				{#if healthStore.bm25.index_path}
					<div class="desc-row">
						<span class="desc-term">Index path</span>
						<span class="desc-value font-mono"
							>{healthStore.bm25.index_path}</span
						>
					</div>
				{/if}
			</div>
		{:else}
			<LoadingState message="BM25 health not loaded" />
		{/if}
	</Card>
</div>

<style>
</style>
