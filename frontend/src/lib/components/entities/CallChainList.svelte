<script lang="ts">
	/**
	 * Ordered list of call chain hops shared by the Call Chain and Call
	 * Path tabs. Each row navigates to the entity detail page on click.
	 */
	import Badge from '$lib/components/ui/Badge.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	interface ChainNode {
		function_id: string;
		function_name: string;
		file_path: string;
		call_line?: number | null;
		relation_type?: string;
	}

	interface Props {
		nodes: ChainNode[];
		onNavigate?: (id: string) => void;
	}

	let { nodes, onNavigate }: Props = $props();

	function navigate(id: string) {
		if (onNavigate) {
			onNavigate(id);
			return;
		}
		goto(resolve(`/entities/${id}`));
	}

	function handleKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			navigate(id);
		}
	}
</script>

<div class="call-chain-list">
	{#each nodes as node, i (node.function_id)}
		<div
			class="chain-item"
			role="button"
			tabindex="0"
			onclick={() => navigate(node.function_id)}
			onkeydown={(e) => handleKeydown(e, node.function_id)}
		>
			<span class="chain-number">{i + 1}</span>
			<div class="chain-content">
				<h4 class="chain-name">{node.function_name}</h4>
				<p class="chain-location">
					{node.file_path}{#if node.call_line}:{node.call_line}{/if}
				</p>
			</div>
			<Badge variant={node.relation_type === 'caller' ? 'default' : 'active'}>
				{node.relation_type === 'caller' ? 'CALLER' : 'CALLEE'}
			</Badge>
		</div>
	{/each}
</div>

<style>
	.call-chain-list {
		display: grid;
		gap: 1rem;
	}

	.chain-item {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 1rem;
		align-items: center;
		padding: 1rem;
		border: 1px solid var(--black);
		cursor: pointer;
		transition: background-color 0.2s;
	}

	.chain-item:hover {
		background-color: var(--gray-100);
	}

	.chain-number {
		font-family: 'Space Mono', monospace;
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--gray-400);
		width: 40px;
		text-align: center;
	}

	.chain-content {
		display: grid;
		gap: 0.25rem;
	}

	.chain-name {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		font-weight: 700;
		margin: 0;
	}

	.chain-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
		margin: 0;
	}
</style>
