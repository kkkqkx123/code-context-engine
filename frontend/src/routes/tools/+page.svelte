<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
	import LazyPanel from '$lib/components/ui/LazyPanel.svelte';

	type ToolTab =
		| 'compress'
		| 'diagnose'
		| 'fold'
		| 'fold-batch'
		| 'symbols'
		| 'references'
		| 'definition'
		| 'batch';

	let activeTab = $state<ToolTab>('compress');

	// Tool props retained across tab switches.
	let diagnoseLanguage = $state('typescript');
	let foldLanguage = $state('rust');
	let symbolFilePath = $state('');
	let symbolLanguage = $state('typescript');
</script>

<svelte:head>
	<title>Tools - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader title="Developer Tools" subtitle="Code analysis utilities and helpers" />

		<Tabs
			variant="underline"
			label="Developer tools"
			active={activeTab}
			tabs={[
				{ id: 'compress', label: 'Code Compression' },
				{ id: 'diagnose', label: 'Code Diagnosis' },
				{ id: 'fold', label: 'File Fold' },
				{ id: 'fold-batch', label: 'Batch Fold' },
				{ id: 'symbols', label: 'Symbol Lookup' },
				{ id: 'references', label: 'References' },
				{ id: 'definition', label: 'Definition' },
				{ id: 'batch', label: 'Batch Compress' },
			]}
			onSelect={(id) => (activeTab = id as ToolTab)}
		/>

		{#if activeTab === 'compress'}
			<Card
				title="Code Compression"
				subtitle="Reduce token count for LLM efficiency"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/CompressionTool.svelte')}
					loadingMessage="Loading compression tool..."
				/>
			</Card>
		{:else if activeTab === 'diagnose'}
			<Card title="Code Diagnosis" subtitle="Analyze code for potential issues">
				<LazyPanel
					loader={() => import('$lib/components/tools/DiagnosisTool.svelte')}
					componentProps={{ language: diagnoseLanguage }}
					loadingMessage="Loading diagnosis tool..."
				/>
			</Card>
		{:else if activeTab === 'fold'}
			<Card
				title="File Fold"
				subtitle="Extract a symbol skeleton from raw text"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/FoldTool.svelte')}
					componentProps={{ language: foldLanguage }}
					loadingMessage="Loading fold tool..."
				/>
			</Card>
		{:else if activeTab === 'fold-batch'}
			<Card
				title="Batch Fold"
				subtitle="Fold multiple raw texts in one request"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/FoldBatchTool.svelte')}
					loadingMessage="Loading batch fold tool..."
				/>
			</Card>
		{:else if activeTab === 'symbols'}
			<Card title="Symbol Lookup" subtitle="Extract and analyze code symbols">
				<LazyPanel
					loader={() => import('$lib/components/tools/SymbolLookupTool.svelte')}
					componentProps={{
						filePath: symbolFilePath,
						language: symbolLanguage,
					}}
					loadingMessage="Loading symbol lookup tool..."
				/>
			</Card>
		{:else if activeTab === 'references'}
			<Card
				title="Find References"
				subtitle="Position-based reference lookup across the project"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/ReferencesTool.svelte')}
					loadingMessage="Loading references tool..."
				/>
			</Card>
		{:else if activeTab === 'definition'}
			<Card
				title="Goto Definition"
				subtitle="Resolve the symbol under a position"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/DefinitionTool.svelte')}
					loadingMessage="Loading definition tool..."
				/>
			</Card>
		{:else}
			<Card
				title="Batch Compress"
				subtitle="Compress multiple files in one request"
			>
				<LazyPanel
					loader={() => import('$lib/components/tools/BatchCompressTool.svelte')}
					loadingMessage="Loading batch compress tool..."
				/>
			</Card>
		{/if}
	</div>
</div>
