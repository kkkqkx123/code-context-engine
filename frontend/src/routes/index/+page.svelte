<script lang="ts">
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { Component } from 'svelte';
	import { onMount } from 'svelte';

	// Lazy load components - only ProjectList loads initially
	let ProjectList: Component | null = $state(null);
	let IndexControl: Component | null = $state(null);
	let FileParser: Component | null = $state(null);
	let IndexMaintenance: Component | null = $state(null);

	let projectListLoaded = $state(false);
	let indexControlLoaded = $state(false);
	let fileParserLoaded = $state(false);
	let indexMaintenanceLoaded = $state(false);

	onMount(async () => {
		// Load ProjectList and IndexControl in parallel after initial render
		const [projectModule, controlModule] = await Promise.all([
			import('$lib/components/index/ProjectList.svelte'),
			import('$lib/components/index/IndexControl.svelte'),
		]);
		ProjectList = projectModule.default;
		IndexControl = controlModule.default;
		projectListLoaded = true;
		indexControlLoaded = true;

		// Load FileParser after primary components
		const parserModule =
			await import('$lib/components/index/FileParser.svelte');
		FileParser = parserModule.default;
		fileParserLoaded = true;

		const maintenanceModule =
			await import('$lib/components/index/IndexMaintenance.svelte');
		IndexMaintenance = maintenanceModule.default;
		indexMaintenanceLoaded = true;
	});
</script>

<svelte:head>
	<title>Index Management - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Index Management"
			subtitle="Manage projects and control code indexing operations"
		/>

		<div class="content-grid">
			{#if projectListLoaded && ProjectList}
				<ProjectList />
			{:else}
				<LoadingState message="Loading project list..." />
			{/if}

			{#if indexControlLoaded && IndexControl}
				<IndexControl />
			{:else}
				<LoadingState message="Loading index control..." />
			{/if}
		</div>

		<div class="parser-section">
			{#if fileParserLoaded && FileParser}
				<FileParser />
			{:else}
				<LoadingState message="Loading file parser..." />
			{/if}
		</div>

		<div class="maintenance-section">
			{#if indexMaintenanceLoaded && IndexMaintenance}
				<IndexMaintenance />
			{:else}
				<LoadingState message="Loading index maintenance..." />
			{/if}
		</div>
	</div>
</div>

<style>
	.content-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 2rem;
		margin-bottom: 3rem;
	}

	.parser-section {
		margin-top: 2rem;
	}

	.maintenance-section {
		margin-top: 2rem;
	}

	@media (max-width: 1024px) {
		.content-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
