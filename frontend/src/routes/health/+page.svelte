<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import UnifiedStatusCard from '$lib/components/health/UnifiedStatusCard.svelte';
	import ServiceDiagnostics from '$lib/components/health/ServiceDiagnostics.svelte';
	import RetryQueueCard from '$lib/components/health/RetryQueueCard.svelte';
	import { onMount } from 'svelte';
	import { healthStore } from '$lib/stores/health.svelte';

	onMount(() => {
		// loadAll is refreshed globally by the layout; also fetch per-service
		// diagnostics so the page shows details beyond the unified summary.
		healthStore.loadAll();
		healthStore.loadQdrantHealth();
		healthStore.loadEmbeddingHealth();
		healthStore.loadBm25Health();
	});
</script>

<svelte:head>
	<title>Health - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Service Health"
			subtitle="External service reachability and retry queue state"
		>
			<Button
				variant="secondary"
				onclick={() => healthStore.loadAll()}
				disabled={healthStore.isLoading}
			>
				Refresh
			</Button>
		</PageHeader>

		{#if healthStore.error}
			<ErrorBanner message={healthStore.error} />
		{/if}

		<UnifiedStatusCard />
		<ServiceDiagnostics />
		<RetryQueueCard />
	</div>
</div>
