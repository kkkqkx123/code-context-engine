<script lang="ts">
	import type { SearchResultItem } from '$lib/api/search';
	import CodeBlock from '../ui/CodeBlock.svelte';
	import Badge from '../ui/Badge.svelte';

	interface Props {
		result: SearchResultItem;
		onNavigate?: (id: string) => void;
	}

	let {
		result,
		onNavigate = () => {}
	}: Props = $props();

	function formatScore(score: number): string {
		return (score * 100).toFixed(1);
	}

	function handleNavigate() {
		onNavigate(String(result.entity_ids[0] ?? ''));
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			handleNavigate();
		}
	}
</script>

<div class="result-card" role="button" tabindex="0" onclick={handleNavigate} onkeydown={handleKeydown}>
	<div class="result-header">
		<div class="result-meta">
			<span class="file-path">{result.file_path}</span>
			<span class="line-numbers">:{result.start_line}{result.end_line ? `-${result.end_line}` : ''}</span>
		</div>
		<div class="result-badges">
			<Badge variant={result.score > 0.8 ? 'active' : 'default'}>
				Score: {formatScore(result.score)}%
			</Badge>
			{#if result.entity_type}
				<Badge variant="default">{result.entity_type}</Badge>
			{/if}
			<Badge variant="default">{result.source}</Badge>
		</div>
	</div>

	{#if result.code_chunk}
		<div class="code-preview">
			<CodeBlock code={result.code_chunk} language="text" />
		</div>
	{/if}

	{#if result.call_chain && result.call_chain.length > 0}
		<div class="call-chain-info">
			<span class="label">Call Chain:</span>
			<span class="value">{result.call_chain.length} nodes</span>
		</div>
	{/if}
</div>

<style>
	/* Clickable primary result unit: black border (per border convention) */
	.result-card {
		border: 1px solid var(--black);
		padding: 1.5rem;
		cursor: pointer;
		transition: background-color 0.2s;
	}

	.result-card:hover {
		background-color: var(--gray-100);
	}

	.result-header {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		margin-bottom: 1rem;
		align-items: start;
	}

	.result-meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.file-path {
		word-break: break-all;
	}

	.line-numbers {
		color: var(--gray-400);
	}

	.result-badges {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.code-preview {
		margin-top: 1rem;
	}

	.call-chain-info {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
		display: flex;
		gap: 0.5rem;
		font-size: 0.85rem;
	}

	.label {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.value {
		font-weight: 700;
	}
</style>
