<script lang="ts">
	import { onMount } from 'svelte';

	// Lazy load components - only ProjectList loads initially
	let ProjectList: any = $state(null);
	let IndexControl: any = $state(null);
	let FileParser: any = $state(null);

	let projectListLoaded = $state(false);
	let indexControlLoaded = $state(false);
	let fileParserLoaded = $state(false);

	onMount(async () => {
		// Load ProjectList and IndexControl in parallel after initial render
		const [projectModule, controlModule] = await Promise.all([
			import('$lib/components/index/ProjectList.svelte'),
			import('$lib/components/index/IndexControl.svelte')
		]);
		ProjectList = projectModule.default;
		IndexControl = controlModule.default;
		projectListLoaded = true;
		indexControlLoaded = true;
		
		// Load FileParser after primary components
		const parserModule = await import('$lib/components/index/FileParser.svelte');
		FileParser = parserModule.default;
		fileParserLoaded = true;
	});
</script>

<svelte:head>
	<title>Index Management - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Index Management</h1>
		<p class="page-description">Manage projects and control code indexing operations</p>

		<div class="content-grid">
			{#if projectListLoaded && ProjectList}
				<ProjectList />
			{:else}
				<div class="loading-placeholder">Loading project list...</div>
			{/if}

			{#if indexControlLoaded && IndexControl}
				<IndexControl />
			{:else}
				<div class="loading-placeholder">Loading index control...</div>
			{/if}
		</div>

		<div class="parser-section">
			{#if fileParserLoaded && FileParser}
				<FileParser />
			{:else}
				<div class="loading-placeholder">Loading file parser...</div>
			{/if}
		</div>
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 3rem;
	}

	.content-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 2rem;
		margin-bottom: 3rem;
	}

	.parser-section {
		margin-top: 2rem;
	}

	.loading-placeholder {
		padding: 2rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
		border: 1px dashed var(--gray-200);
	}

	@media (max-width: 1024px) {
		.content-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
