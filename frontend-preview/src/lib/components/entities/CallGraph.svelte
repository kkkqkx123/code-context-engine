<script lang="ts">
	import type { CallChainNode } from '$lib/api/search';

	interface Props {
		nodes?: CallChainNode[];
		onNavigate?: (id: string) => void;
	}

	let {
		nodes = [],
		onNavigate = () => {}
	}: Props = $props();

	let nodePositions = $derived(calculatePositions(nodes));

	function calculatePositions(nodes: CallChainNode[]): Array<{ x: number; y: number }> {
		if (nodes.length === 0) return [];

		const positions: Array<{ x: number; y: number }> = [];
		const spacing = 180;
		const centerX = 400;

		nodes.forEach((_, index) => {
			positions.push({
				x: centerX,
				y: 80 + index * spacing,
			});
		});

		return positions;
	}

	function getNodeWidth(text: string): number {
		return Math.max(200, text.length * 8 + 40);
	}

	function handleNodeKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onNavigate(id);
		}
	}
</script>

<div class="call-graph-container">
	{#if nodes.length === 0}
		<div class="empty-state">
			<p>No call graph data available</p>
		</div>
	{:else}
		<svg class="graph-svg" viewBox="0 0 800 {Math.max(600, nodes.length * 180 + 100)}">
			<!-- Draw edges -->
			{#each nodes.slice(0, -1) as node, i}
				{#if i < nodes.length - 1}
					{@const startPos = nodePositions[i]}
					{@const endPos = nodePositions[i + 1]}
					{@const edgeLength = Math.sqrt(Math.pow(endPos.x - startPos.x, 2) + Math.pow(endPos.y - startPos.y, 2))}
					{@const angle = Math.atan2(endPos.y - startPos.y, endPos.x - startPos.x)}

					<line
						class="edge"
						x1={startPos.x}
						y1={startPos.y + 30}
						x2={endPos.x}
						y2={endPos.y - 30}
						style="stroke-dasharray: {edgeLength}; stroke-dashoffset: {edgeLength}; animation: drawEdge 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {i * 0.1}s;"
					/>

					<!-- Arrow head -->
					<polygon
						points="{endPos.x},{endPos.y - 30} {endPos.x - 8},{endPos.y - 45} {endPos.x + 8},{endPos.y - 45}"
						transform="rotate({(angle * 180) / Math.PI + 90}, {endPos.x}, {endPos.y - 30})"
						class="arrow-head"
					/>
				{/if}
			{/each}

			<!-- Draw nodes -->
			{#each nodes as node, i}
				{@const pos = nodePositions[i]}
				{@const width = getNodeWidth(node.function_name)}
				<g
					class="node-group"
					role="button"
					tabindex="0"
					onclick={() => onNavigate(node.function_id)}
					onkeydown={(e) => handleNodeKeydown(e, node.function_id)}
					style="opacity: 0; transform: translateY(10px); animation: popIn 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {i * 0.1}s;"
				>
					<rect
						class="node-rect"
						x={pos.x - width / 2}
						y={pos.y - 30}
						width={width}
						height={60}
					/>
					<text
						class="node-text"
						x={pos.x}
						y={pos.y}
						text-anchor="middle"
						dominant-baseline="middle"
					>
						{node.function_name}
					</text>
					<text
						class="node-meta"
						x={pos.x}
						y={pos.y + 18}
						text-anchor="middle"
						dominant-baseline="middle"
					>
						{node.file_path.split('/').pop()}:{node.call_line ?? ''}
					</text>
				</g>
			{/each}
		</svg>

		<div class="graph-legend">
			<div class="legend-item">
				<span class="legend-box"></span>
				<span>Caller/Callee</span>
			</div>
		</div>
	{/if}
</div>

<style>
	.call-graph-container {
		border: 1px solid var(--gray-200);
		padding: 2rem;
		overflow-x: auto;
	}

	.empty-state {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}

	.graph-svg {
		width: 100%;
		min-height: 400px;
		display: block;
	}

	.edge {
		stroke: var(--black);
		stroke-width: 2;
		fill: none;
	}

	.arrow-head {
		fill: var(--black);
	}

	.node-group {
		cursor: pointer;
	}

	.node-rect {
		fill: var(--white);
		stroke: var(--black);
		stroke-width: 2;
		transition: fill 0.2s;
	}

	.node-group:hover .node-rect {
		fill: var(--gray-100);
	}

	.node-text {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 14px;
		font-weight: 700;
		fill: var(--black);
	}

	.node-meta {
		font-family: 'Space Mono', monospace;
		font-size: 10px;
		fill: var(--gray-600);
	}

	.graph-legend {
		margin-top: 1.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--gray-200);
		display: flex;
		gap: 2rem;
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.85rem;
	}

	.legend-box {
		width: 20px;
		height: 20px;
		border: 2px solid var(--black);
		background: var(--white);
	}

	@keyframes drawEdge {
		to {
			stroke-dashoffset: 0;
		}
	}

	@keyframes popIn {
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
