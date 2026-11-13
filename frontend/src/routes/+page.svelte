<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { metricsStore } from '$lib/stores/metrics.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import KpiCard from '$lib/components/ui/KpiCard.svelte';

	const RECENT_KEY = 'cce.recentQueries';
	const EXAMPLES = [
		'authentication function',
		'retry logic',
		'database connection',
		'error handling',
	];

	let homeQuery = $state('');
	let recentQueries = $state<string[]>([]);

	onMount(() => {
		try {
			const raw = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
			if (Array.isArray(raw)) {
				recentQueries = raw
					.filter((q): q is string => typeof q === 'string')
					.slice(0, 6);
			}
		} catch {
			// Storage may be unavailable; recent queries simply stay empty.
		}
	});

	function runSearch(query: string) {
		const trimmed = query.trim();
		if (!trimmed) return;
		recentQueries = [
			trimmed,
			...recentQueries.filter((q) => q !== trimmed),
		].slice(0, 6);
		try {
			localStorage.setItem(RECENT_KEY, JSON.stringify(recentQueries));
		} catch {
			// Ignore storage failures; navigation still works.
		}
		goto(resolve(`/search?q=${encodeURIComponent(trimmed)}`));
	}

	let vectorCount = $derived(
		metricsStore.storageStatus?.vector_storage.item_count ?? 0,
	);
	let bm25Count = $derived(
		metricsStore.storageStatus?.bm25_storage.item_count ?? 0,
	);
	let serverOk = $derived(
		metricsStore.lastUpdated != null && !metricsStore.error,
	);
	let lastUpdatedText = $derived(
		metricsStore.lastUpdated
			? metricsStore.lastUpdated.toLocaleTimeString()
			: '—',
	);
</script>

<svelte:head>
	<title>Dashboard - Code Context Engine</title>
</svelte:head>

<div class="page">
	<PageHeader
		title="Dashboard"
		subtitle="Code indexing, search, and analysis console"
	/>

	<div class="home-search">
		<input
			class="search-hero"
			type="text"
			placeholder="Search the indexed codebase…"
			aria-label="Search the indexed codebase"
			bind:value={homeQuery}
			onkeydown={(e) => {
				if (e.key === 'Enter') runSearch(homeQuery);
			}}
		/>
		<Button
			variant="primary"
			onclick={() => runSearch(homeQuery)}
			disabled={!homeQuery.trim()}
		>
			Search
		</Button>
	</div>

	<div class="home-suggestions">
		{#if recentQueries.length > 0}
			<span class="suggest-label">Recent:</span>
			{#each recentQueries as query (query)}
				<button
					type="button"
					class="suggest-chip"
					onclick={() => runSearch(query)}
				>
					{query}
				</button>
			{/each}
		{/if}
		<span class="suggest-label">Try:</span>
		{#each EXAMPLES as example (example)}
			<button
				type="button"
				class="suggest-chip"
				onclick={() => runSearch(example)}
			>
				{example}
			</button>
		{/each}
	</div>

	<h2 class="section-heading">System status</h2>

	<div class="kpi-grid">
		<KpiCard
			label="Projects"
			value={String(projectStore.projects.length)}
			meta="indexed codebases"
		/>
		<KpiCard
			label="Vectors"
			value={String(vectorCount)}
			meta="embeddings stored"
		/>
		<KpiCard
			label="BM25 Docs"
			value={String(bm25Count)}
			meta="full-text index"
		/>
		<KpiCard label="Server" meta={`updated ${lastUpdatedText}`}>
			<Badge
				label={serverOk ? 'OK' : 'DOWN'}
				variant={serverOk ? 'success' : 'danger'}
			/>
		</KpiCard>
	</div>

	<div class="quick-grid">
		<a
			class="quick-tile"
			href={resolve('/projects')}
			onclick={(e) => {
				e.preventDefault();
				goto(resolve('/projects'));
			}}
		>
			<h3>Project Workbench</h3>
			<p>Create projects, trigger indexing, maintain the index</p>
		</a>
		<a
			class="quick-tile"
			href={resolve('/search')}
			onclick={(e) => {
				e.preventDefault();
				goto(resolve('/search'));
			}}
		>
			<h3>Search Code</h3>
			<p>Semantic and keyword-based code search</p>
		</a>
		<a
			class="quick-tile"
			href={resolve('/search?view=entities')}
			onclick={(e) => {
				e.preventDefault();
				goto(resolve('/search?view=entities'));
			}}
		>
			<h3>Explore Entities</h3>
			<p>Browse functions, classes, and relationships</p>
		</a>
		<a
			class="quick-tile"
			href={resolve('/graph')}
			onclick={(e) => {
				e.preventDefault();
				goto(resolve('/graph'));
			}}
		>
			<h3>Graph Explorer</h3>
			<p>Visualize calls, dependencies, and impact analysis</p>
		</a>
		<a
			class="quick-tile"
			href={resolve('/storage')}
			onclick={(e) => {
				e.preventDefault();
				goto(resolve('/storage'));
			}}
		>
			<h3>Manage Storage</h3>
			<p>View statistics and clean up indexes</p>
		</a>
	</div>
</div>

<style>
	.home-search {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		margin-bottom: 0.75rem;
	}

	.home-suggestions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 2.5rem;
	}

	.suggest-label {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
	}
</style>
