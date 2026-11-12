<script lang="ts">
	import type { FunctionInfo } from '$lib/api/entities';
	import type { CallChainNode } from '$lib/api/search';
	import CodeBlock from '../ui/CodeBlock.svelte';
	import Badge from '../ui/Badge.svelte';

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

	function handleItemKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			handleNavigate(id);
		}
	}
</script>

<div class="entity-detail">
	{#if func}
		<div class="entity-header">
			<h2 class="entity-name">{func.name}</h2>
			<div class="entity-badges">
				<Badge variant="active">FUNCTION</Badge>
			</div>
		</div>

		<div class="entity-meta">
			<div class="meta-item">
				<span class="label">File:</span>
				<span class="value">{func.file_path}</span>
			</div>
			<div class="meta-item">
				<span class="label">Lines:</span>
				<span class="value">{func.start_line}-{func.end_line}</span>
			</div>
		</div>

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
			<div class="relationship-list">
				{#each callers.slice(0, showAllCallers ? callers.length : 10) as caller (caller.function_id)}
					<div
						class="relationship-item"
						onclick={() => handleNavigate(caller.function_id)}
						onkeydown={(e) => handleItemKeydown(e, caller.function_id)}
						tabindex="0"
						role="button"
					>
						<span class="item-name">{caller.function_name}</span>
						<span class="item-location"
							>{caller.file_path}:{caller.call_line ?? ''}</span
						>
					</div>
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
			</div>
		</div>
	{/if}

	{#if calls && calls.length > 0}
		<div class="relationships-section">
			<h3 class="eyebrow-title">Calls ({calls.length})</h3>
			<div class="relationship-list">
				{#each calls.slice(0, allCallees ? calls.length : 10) as call (call.function_id)}
					<div
						class="relationship-item"
						onclick={() => handleNavigate(call.function_id)}
						onkeydown={(e) => handleItemKeydown(e, call.function_id)}
						tabindex="0"
						role="button"
					>
						<span class="item-name">{call.function_name}</span>
						<span class="item-location"
							>{call.file_path}:{call.call_line ?? ''}</span
						>
					</div>
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
			</div>
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
		border-bottom: 1px solid var(--gray-200);
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

	.entity-meta {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 1rem;
	}

	.meta-item {
		display: grid;
		gap: 0.25rem;
	}

	.label {
		font-family: var(--font-mono);
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: var(--tracking-widest);
		color: var(--gray-600);
	}

	.value {
		font-family: var(--font-mono);
		font-size: 0.85rem;
		word-break: break-all;
	}


	.signature-section,
	.description-section,
	.relationships-section {
		border-top: 1px solid var(--gray-200);
		padding-top: 1.5rem;
	}

	.description-text {
		font-family: var(--font-sans);
		font-size: 1rem;
		line-height: 1.6;
		color: var(--gray-600);
	}

	.relationship-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 0.5rem;
	}

	/* Clickable relationship item: black border (per border convention) */
	.relationship-item {
		padding: 0.75rem;
		border: 1px solid var(--black);
		cursor: pointer;
		transition: background-color 0.2s;
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		align-items: center;
	}

	.relationship-item:hover {
		background-color: var(--gray-100);
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

	.more-toggle {
		width: 100%;
		padding: 0.75rem;
		background: transparent;
		border: 1px dashed var(--gray-300);
		cursor: pointer;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.more-toggle:hover {
		background: var(--gray-100);
	}
</style>
