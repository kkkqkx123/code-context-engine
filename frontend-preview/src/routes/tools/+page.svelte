<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { Component } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';

	// Tab state
	let activeTab = $state<
		| 'compress'
		| 'diagnose'
		| 'fold'
		| 'symbols'
		| 'references'
		| 'definition'
		| 'keyword'
		| 'batch'
	>('compress');

	// Lazy loaded components
	let CompressionTool: Component | null = $state(null);
	let DiagnosisTool: Component | null = $state(null);
	let FoldTool: Component | null = $state(null);
	let SymbolLookupTool: Component | null = $state(null);
	let ReferencesTool: Component | null = $state(null);
	let DefinitionTool: Component | null = $state(null);
	let KeywordSearchTool: Component | null = $state(null);
	let BatchCompressTool: Component | null = $state(null);

	// Component props
	let compressLanguage = $state('typescript');
	let diagnoseLanguage = $state('typescript');
	let foldLanguage = $state('rust');
	let symbolFilePath = $state('');
	let symbolLanguage = $state('typescript');

	// Load components on-demand
	async function loadCompressionTool() {
		if (!CompressionTool) {
			const module =
				await import('$lib/components/tools/CompressionTool.svelte');
			CompressionTool = module.default;
		}
	}

	async function loadDiagnosisTool() {
		if (!DiagnosisTool) {
			const module = await import('$lib/components/tools/DiagnosisTool.svelte');
			DiagnosisTool = module.default;
		}
	}

	async function loadFoldTool() {
		if (!FoldTool) {
			const module = await import('$lib/components/tools/FoldTool.svelte');
			FoldTool = module.default;
		}
	}

	async function loadSymbolLookupTool() {
		if (!SymbolLookupTool) {
			const module =
				await import('$lib/components/tools/SymbolLookupTool.svelte');
			SymbolLookupTool = module.default;
		}
	}

	async function loadReferencesTool() {
		if (!ReferencesTool) {
			const module =
				await import('$lib/components/tools/ReferencesTool.svelte');
			ReferencesTool = module.default;
		}
	}

	async function loadDefinitionTool() {
		if (!DefinitionTool) {
			const module =
				await import('$lib/components/tools/DefinitionTool.svelte');
			DefinitionTool = module.default;
		}
	}

	async function loadKeywordSearchTool() {
		if (!KeywordSearchTool) {
			const module =
				await import('$lib/components/tools/KeywordSearchTool.svelte');
			KeywordSearchTool = module.default;
		}
	}

	async function loadBatchCompressTool() {
		if (!BatchCompressTool) {
			const module =
				await import('$lib/components/tools/BatchCompressTool.svelte');
			BatchCompressTool = module.default;
		}
	}

	// Watch for tab changes and load components
	$effect(() => {
		if (activeTab === 'compress') {
			loadCompressionTool();
		} else if (activeTab === 'diagnose') {
			loadDiagnosisTool();
		} else if (activeTab === 'fold') {
			loadFoldTool();
		} else if (activeTab === 'symbols') {
			loadSymbolLookupTool();
		} else if (activeTab === 'references') {
			loadReferencesTool();
		} else if (activeTab === 'definition') {
			loadDefinitionTool();
		} else if (activeTab === 'keyword') {
			loadKeywordSearchTool();
		} else if (activeTab === 'batch') {
			loadBatchCompressTool();
		}
	});
</script>

<svelte:head>
	<title>Tools - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Developer Tools"
			subtitle="Code analysis utilities and helpers"
		/>

		<!-- Tab Navigation -->
		<div class="tab-nav">
			<button
				class="tab-btn"
				class:active={activeTab === 'compress'}
				onclick={() => (activeTab = 'compress')}
			>
				Code Compression
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'diagnose'}
				onclick={() => (activeTab = 'diagnose')}
			>
				Code Diagnosis
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'fold'}
				onclick={() => (activeTab = 'fold')}
			>
				File Fold
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'symbols'}
				onclick={() => (activeTab = 'symbols')}
			>
				Symbol Lookup
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'references'}
				onclick={() => (activeTab = 'references')}
			>
				References
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'definition'}
				onclick={() => (activeTab = 'definition')}
			>
				Definition
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'keyword'}
				onclick={() => (activeTab = 'keyword')}
			>
				Keyword Search
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'batch'}
				onclick={() => (activeTab = 'batch')}
			>
				Batch Compress
			</button>
		</div>

		<!-- Code Compression Tool -->
		{#if activeTab === 'compress'}
			<Card
				title="Code Compression"
				subtitle="Reduce token count for LLM efficiency"
			>
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

		<!-- File Fold Tool -->
		{#if activeTab === 'fold'}
			<Card
				title="File Fold"
				subtitle="Extract a symbol skeleton from raw text"
			>
				{#if FoldTool}
					<FoldTool language={foldLanguage} />
				{:else}
					<div class="loading-placeholder">Loading fold tool...</div>
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

		<!-- References Tool -->
		{#if activeTab === 'references'}
			<Card
				title="Find References"
				subtitle="Position-based reference lookup across the project"
			>
				{#if ReferencesTool}
					<ReferencesTool />
				{:else}
					<div class="loading-placeholder">Loading references tool...</div>
				{/if}
			</Card>
		{/if}

		<!-- Definition Tool -->
		{#if activeTab === 'definition'}
			<Card
				title="Goto Definition"
				subtitle="Resolve the symbol under a position"
			>
				{#if DefinitionTool}
					<DefinitionTool />
				{:else}
					<div class="loading-placeholder">Loading definition tool...</div>
				{/if}
			</Card>
		{/if}

		<!-- Keyword Search Tool -->
		{#if activeTab === 'keyword'}
			<Card
				title="Keyword Search"
				subtitle="BM25 keyword search over indexed chunks"
			>
				{#if KeywordSearchTool}
					<KeywordSearchTool />
				{:else}
					<div class="loading-placeholder">Loading keyword search tool...</div>
				{/if}
			</Card>
		{/if}

		<!-- Batch Compress Tool -->
		{#if activeTab === 'batch'}
			<Card
				title="Batch Compress"
				subtitle="Compress multiple files in one request"
			>
				{#if BatchCompressTool}
					<BatchCompressTool />
				{:else}
					<div class="loading-placeholder">Loading batch compress tool...</div>
				{/if}
			</Card>
		{/if}
	</div>
</div>

<style>
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
