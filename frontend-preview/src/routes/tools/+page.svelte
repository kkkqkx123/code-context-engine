<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';

	// Tab state
	let activeTab = $state<'compress' | 'diagnose' | 'symbols'>('compress');

	// Lazy loaded components
	let CompressionTool: any = $state(null);
	let DiagnosisTool: any = $state(null);
	let SymbolLookupTool: any = $state(null);

	// Component props
	let compressLanguage = $state('typescript');
	let diagnoseLanguage = $state('typescript');
	let symbolFilePath = $state('');
	let symbolLanguage = $state('typescript');

	// Load components on-demand
	async function loadCompressionTool() {
		if (!CompressionTool) {
			const module = await import('$lib/components/tools/CompressionTool.svelte');
			CompressionTool = module.default;
		}
	}

	async function loadDiagnosisTool() {
		if (!DiagnosisTool) {
			const module = await import('$lib/components/tools/DiagnosisTool.svelte');
			DiagnosisTool = module.default;
		}
	}

	async function loadSymbolLookupTool() {
		if (!SymbolLookupTool) {
			const module = await import('$lib/components/tools/SymbolLookupTool.svelte');
			SymbolLookupTool = module.default;
		}
	}

	// Watch for tab changes and load components
	$effect(() => {
		if (activeTab === 'compress') {
			loadCompressionTool();
		} else if (activeTab === 'diagnose') {
			loadDiagnosisTool();
		} else if (activeTab === 'symbols') {
			loadSymbolLookupTool();
		}
	});
</script>

<svelte:head>
	<title>Tools - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Developer Tools</h1>
		<p class="page-description">Code analysis utilities and helpers</p>

		<!-- Tab Navigation -->
		<div class="tab-nav">
			<button
				class="tab-btn"
				class:active={activeTab === 'compress'}
				onclick={() => activeTab = 'compress'}
			>
				Code Compression
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'diagnose'}
				onclick={() => activeTab = 'diagnose'}
			>
				Code Diagnosis
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'symbols'}
				onclick={() => activeTab = 'symbols'}
			>
				Symbol Lookup
			</button>
		</div>

		<!-- Code Compression Tool -->
		{#if activeTab === 'compress'}
			<Card title="Code Compression" subtitle="Reduce token count for LLM efficiency">
				{#if CompressionTool}
					<CompressionTool language={compressLanguage} />
				{:else}
					<div class="loading-placeholder">Loading compression tool...</div>
				{/if}
			</Card>
		{/if}

		<!-- Code Diagnosis Tool -->
		{#if activeTab === 'diagnose'}
			<Card title="Code Diagnosis" subtitle="Analyze code for potential issues">
				{#if DiagnosisTool}
					<DiagnosisTool language={diagnoseLanguage} />
				{:else}
					<div class="loading-placeholder">Loading diagnosis tool...</div>
				{/if}
			</Card>
		{/if}

		<!-- Symbol Lookup Tool -->
		{#if activeTab === 'symbols'}
			<Card title="Symbol Lookup" subtitle="Extract and analyze code symbols">
				{#if SymbolLookupTool}
					<SymbolLookupTool
						filePath={symbolFilePath}
						language={symbolLanguage}
					/>
				{:else}
					<div class="loading-placeholder">Loading symbol lookup tool...</div>
				{/if}
			</Card>
		{/if}
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 2rem;
	}

	/* Tab Navigation */
	.tab-nav {
		display: flex;
		gap: 0;
		border-bottom: 2px solid var(--black);
		margin-bottom: 2rem;
	}

	.tab-btn {
		padding: 1rem 2rem;
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -2px;
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		transition: all 0.3s ease;
		color: var(--gray-600);
	}

	.tab-btn:hover {
		color: var(--black);
	}

	.tab-btn.active {
		color: var(--black);
		border-bottom-color: var(--accent);
		font-weight: bold;
	}

	.loading-placeholder {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}
</style>
