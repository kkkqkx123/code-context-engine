<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { metricsState, metricsActions } from '$lib/stores/metrics';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';

	onMount(() => {
		// Start auto-refresh with 30s interval
		metricsActions.startAutoRefresh(30000);
	});

	onDestroy(() => {
		// Stop auto-refresh when component is destroyed
		metricsActions.stopAutoRefresh();
	});
</script>

<svelte:head>
	<title>Dashboard - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<div class="hero">
			<h1>Code Context Engine</h1>
			<p class="hero-subtitle">Web-based interface for code indexing, search, and analysis</p>
		</div>

		<div class="status-grid">
			<Card title="Server Status" subtitle="Connection Health">
				<div class="status-indicator">
					{#if $metricsState.isLoading}
						<Badge variant="inactive">Loading...</Badge>
					{:else if $metricsState.error}
						<Badge variant="active">Error</Badge>
						<p class="error-text">{$metricsState.error}</p>
					{:else}
						<Badge variant="active">Connected</Badge>
						{#if $metricsState.lastUpdated}
							<p class="meta-text">
								Last updated: {$metricsState.lastUpdated.toLocaleTimeString()}
							</p>
						{/if}
					{/if}
				</div>
			</Card>

			<Card title="Storage Components" subtitle="System Health">
				{#if $metricsState.storageStatus}
					<div class="component-list">
						<div class="component-item">
							<span class="component-name">Vector DB</span>
							<Badge
								variant={$metricsState.storageStatus.vector_storage.connected ? 'active' : 'inactive'}
							>
								{$metricsState.storageStatus.vector_storage.connected ? 'Connected' : 'Disconnected'}
							</Badge>
						</div>
						<div class="component-item">
							<span class="component-name">BM25 Index</span>
							<Badge
								variant={$metricsState.storageStatus.bm25_storage.connected ? 'active' : 'inactive'}
							>
								{$metricsState.storageStatus.bm25_storage.connected ? 'Connected' : 'Disconnected'}
							</Badge>
						</div>
						<div class="component-item">
							<span class="component-name">Relations</span>
							<Badge
								variant={$metricsState.storageStatus.relation_storage.connected ? 'active' : 'inactive'}
							>
								{$metricsState.storageStatus.relation_storage.connected ? 'Connected' : 'Disconnected'}
							</Badge>
						</div>
						<div class="component-item">
							<span class="component-name">Cache</span>
							<Badge
								variant={$metricsState.storageStatus.cache_storage.connected ? 'active' : 'inactive'}
							>
								{$metricsState.storageStatus.cache_storage.connected ? 'Connected' : 'Disconnected'}
							</Badge>
						</div>
					</div>
				{:else}
					<p class="placeholder-text">No data available</p>
				{/if}
			</Card>
		</div>

		<div class="quick-actions">
			<h2>Quick Actions</h2>
			<div class="action-grid">
				<a href="/index" class="action-card">
					<h3>Manage Index</h3>
					<p>Create projects, trigger indexing, monitor progress</p>
				</a>
				<a href="/search" class="action-card">
					<h3>Search Code</h3>
					<p>Semantic and keyword-based code search</p>
				</a>
				<a href="/entities" class="action-card">
					<h3>Explore Entities</h3>
					<p>Browse functions, classes, and relationships</p>
				</a>
				<a href="/storage" class="action-card">
					<h3>Manage Storage</h3>
					<p>View statistics and clean up indexes</p>
				</a>
			</div>
		</div>
	</div>
</section>

<style>
	.hero {
		margin-bottom: 4rem;
	}

	.hero h1 {
		font-size: clamp(3rem, 6vw, 5rem);
		margin-bottom: 1rem;
	}

	.hero-subtitle {
		font-size: 1.25rem;
		color: var(--gray-600);
		max-width: 600px;
	}

	.status-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 2rem;
		margin-bottom: 4rem;
	}

	.status-indicator {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.error-text {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--danger);
	}

	.meta-text {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.component-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.component-item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--gray-200);
	}

	.component-name {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 500;
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
	}

	.quick-actions {
		margin-top: 4rem;
	}

	.quick-actions h2 {
		margin-bottom: 2rem;
	}

	.action-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 2rem;
	}

	.action-card {
		display: block;
		padding: 2rem;
		border: 1px solid var(--black);
		text-decoration: none;
		color: inherit;
		transition: background 0.3s;
	}

	.action-card:hover {
		background: var(--gray-100);
	}

	.action-card h3 {
		font-size: 1.5rem;
		margin-bottom: 0.75rem;
	}

	.action-card p {
		color: var(--gray-600);
		line-height: 1.6;
	}

	@media (max-width: 1024px) {
		.status-grid,
		.action-grid {
			grid-template-columns: 1fr;
		}
	}

	@media (max-width: 768px) {
	}
</style>
