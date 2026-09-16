<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { metricsState, metricsActions } from '$lib/stores/metrics';
	import { projects } from '$lib/stores/index';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';

	onMount(() => {
		metricsActions.startAutoRefresh(30000);
	});

	onDestroy(() => {
		metricsActions.stopAutoRefresh();
	});

	let vectorCount = $derived($metricsState.storageStatus?.vector_storage.item_count ?? 0);
	let bm25Count = $derived($metricsState.storageStatus?.bm25_storage.item_count ?? 0);
	let serverOk = $derived($metricsState.lastUpdated != null && !$metricsState.error);
	let lastUpdatedText = $derived(
		$metricsState.lastUpdated ? $metricsState.lastUpdated.toLocaleTimeString() : '—'
	);
</script>

<svelte:head>
	<title>Dashboard - Code Context Engine</title>
</svelte:head>

<div class="page">
	<PageHeader title="Dashboard" subtitle="Code indexing, search, and analysis console" />

	<div class="kpi-grid">
		<div class="kpi-card">
			<span class="kpi-label">Projects</span>
			<span class="kpi-value">{$projects.length}</span>
			<span class="kpi-meta">indexed codebases</span>
		</div>
		<div class="kpi-card">
			<span class="kpi-label">Vectors</span>
			<span class="kpi-value">{vectorCount}</span>
			<span class="kpi-meta">embeddings stored</span>
		</div>
		<div class="kpi-card">
			<span class="kpi-label">BM25 Docs</span>
			<span class="kpi-value">{bm25Count}</span>
			<span class="kpi-meta">full-text index</span>
		</div>
		<div class="kpi-card">
			<span class="kpi-label">Server</span>
			<span class="kpi-value">{serverOk ? 'OK' : 'DOWN'}</span>
			<span class="kpi-meta">updated {lastUpdatedText}</span>
		</div>
	</div>

	<div class="quick-grid">
		<a class="quick-tile" href="/index">
			<h3>Manage Index</h3>
			<p>Create projects, trigger indexing, monitor progress</p>
		</a>
		<a class="quick-tile" href="/search">
			<h3>Search Code</h3>
			<p>Semantic and keyword-based code search</p>
		</a>
		<a class="quick-tile" href="/entities">
			<h3>Explore Entities</h3>
			<p>Browse functions, classes, and relationships</p>
		</a>
		<a class="quick-tile" href="/storage">
			<h3>Manage Storage</h3>
			<p>View statistics and clean up indexes</p>
		</a>
	</div>
</div>
