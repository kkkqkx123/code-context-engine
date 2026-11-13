<script lang="ts">
	import type { FunctionInfo } from '$lib/api/entities';
	import type { CallChainNode } from '$lib/api/search';
	import CodeBlock from '../ui/CodeBlock.svelte';
	import Badge from '../ui/Badge.svelte';
	import DescriptionList from '../ui/DescriptionList.svelte';

	interface Props {
		func?: FunctionInfo | null;
		calls?: CallChainNode[];
		callers?: CallChainNode[];
		onNavigate?: (id: string) => void;
	}

	let {
		func = null,
		calls = [],
		callers = [],
		onNavigate = () => {},
	}: Props = $props();

	function handleNavigate(id: string) {
		onNavigate(id);
	}

	let showAllCallers = $state(false);
	let allCallees = $state(false);
</script>

<div class="entity-detail">
	{#if func}
		<div class="entity-header">
			<h2 class="entity-name">{func.name}</h2>
			<div class="entity-badges">
				<Badge variant="active">FUNCTION</Badge>
			</div>
		</div>

		<DescriptionList
			label="Entity location"
			items={[
				{ term: 'File', value: func.file_path, mono: true },
				{
					term: 'Lines',
					value: `${func.start_line}-${func.end_line}`,
					mono: true,
				},
			]}
		/>

		{#if func.signature}
			<div class="signature-section">
				<h3 class="eyebrow-title">Signature</h3>
				<CodeBlock code={func.signature} language="text" />
			</div>
		{/if}

		{#if func.doc_comment}
			<div class="description-section">
				<h3 class="eyebrow-title">Description</h3>
				<p class="description-text">{func.doc_comment}</p>
			</div>
		{/if}
	{/if}

	{#if callers && callers.length > 0}
		<div class="relationships-section">
			<h3 class="eyebrow-title">Called By ({callers.length})</h3>
			<ul class="scroll-list">
				{#each callers.slice(0, showAllCallers ? callers.length : 10) as caller (caller.function_id)}
					<li>
						<button
							type="button"
							class="selectable-row"
							onclick={() => handleNavigate(caller.function_id)}
						>
							<span class="item-name">{caller.function_name}</span>
							<span class="item-location"
								>{caller.file_path}:{caller.call_line ?? ''}</span
							>
						</button>
					</li>
				{/each}
				{#if callers.length > 10}
					<button
						type="button"
						class="more-toggle"
						onclick={() => (showAllCallers = !showAllCallers)}
					>
						{showAllCallers
							? 'Show fewer'
							: `+${callers.length - 10} more — show all`}
					</button>
				{/if}
			</ul>
		</div>
	{/if}

	{#if calls && calls.length > 0}
		<div class="relationships-section">
			<h3 class="eyebrow-title">Calls ({calls.length})</h3>
			<ul class="scroll-list">
				{#each calls.slice(0, allCallees ? calls.length : 10) as call (call.function_id)}
					<li>
						<button
							type="button"
							class="selectable-row"
							onclick={() => handleNavigate(call.function_id)}
						>
							<span class="item-name">{call.function_name}</span>
							<span class="item-location"
								>{call.file_path}:{call.call_line ?? ''}</span
							>
						</button>
					</li>
				{/each}
				{#if calls.length > 10}
					<button
						type="button"
						class="more-toggle"
						onclick={() => (allCallees = !allCallees)}
					>
						{allCallees
							? 'Show fewer'
							: `+${calls.length - 10} more — show all`}
					</button>
				{/if}
			</ul>
		</div>
	{/if}
</div>

<style>
	.entity-detail {
		display: grid;
		gap: 2rem;
	}

	.entity-header {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		align-items: start;
		padding-bottom: 1.5rem;
		border-bottom: var(--border-muted);
	}

	.entity-name {
		font-family: var(--font-sans);
		font-size: 1.75rem;
		font-weight: 700;
		letter-spacing: var(--tracking-tight);
		line-height: 1.05;
		margin: 0;
	}

	.entity-badges {
		display: flex;
		gap: 0.5rem;
	}

	.signature-section,
	.description-section,
	.relationships-section {
		border-top: var(--border-muted);
		padding-top: 1.5rem;
	}

	.description-text {
		font-family: var(--font-sans);
		font-size: 1rem;
		line-height: 1.6;
		color: var(--gray-600);
	}

	.item-name {
		font-family: var(--font-sans);
		font-weight: 700;
	}

	.item-location {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--gray-400);
	}
</style>
