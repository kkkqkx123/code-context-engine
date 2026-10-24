<script lang="ts">
	/**
	 * Static legend for the graph explorer: line style per relation domain
	 * and node shape per node kind. Purely presentational.
	 */
	import { NODE_SHAPE_LEGEND, RELATION_DOMAINS, relationLineStyle } from '$lib/utils/graph-domains';
</script>

<div class="legend">
	{#each Object.values(RELATION_DOMAINS) as domain (domain.domain)}
		<div class="legend-item">
			<span
				class="legend-line"
				style="--line: {domain.color}; --line-style: {relationLineStyle(
					domain.domain,
				)}"
			></span>
			<span class="legend-label">{domain.label}</span>
		</div>
	{/each}
	<div class="legend-separator">Nodes</div>
	{#each NODE_SHAPE_LEGEND as entry (entry.label)}
		<div class="legend-item">
			<span
				class="legend-node"
				data-shape={entry.shape}
				class:external={entry.external}
			></span>
			<span class="legend-label">{entry.label}</span>
		</div>
	{/each}
</div>

<style>
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 1.5rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.legend-line {
		width: 22px;
		border-top-width: 2px;
		border-top-color: var(--line);
		border-top-style: var(--line-style, solid);
	}

	.legend-separator {
		width: 100%;
		margin-top: 0.35rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
	}

	/* Mirrors the Cytoscape silhouettes closely enough to read as the same
	   vocabulary; polygons CSS cannot express with a radius are clipped
	   instead. */
	.legend-node {
		width: 14px;
		height: 14px;
		border: 1.5px solid var(--black);
		background: var(--white);
		box-sizing: border-box;
	}

	.legend-node[data-shape='rectangle'] {
		border-radius: 0;
		background: #f5f3ff;
		border-color: #5b21b6;
	}

	.legend-node[data-shape='round-rectangle'] {
		border-radius: 4px;
		background: #eff6ff;
		border-color: #1e40af;
	}

	.legend-node[data-shape='hexagon'] {
		clip-path: polygon(25% 0, 75% 0, 100% 50%, 75% 100%, 25% 100%, 0 50%);
		background: #ecfdf5;
		border-color: #065f46;
	}

	.legend-node[data-shape='ellipse'] {
		border-radius: 50%;
		background: #fffbeb;
		border-color: #92400e;
	}

	.legend-node[data-shape='octagon'] {
		clip-path: polygon(
			30% 0,
			70% 0,
			100% 30%,
			100% 70%,
			70% 100%,
			30% 100%,
			0 70%,
			0 30%
		);
		background: #f8fafc;
		border-color: #334155;
	}

	.legend-node[data-shape='pentagon'] {
		clip-path: polygon(50% 0, 100% 38%, 82% 100%, 18% 100%, 0 38%);
	}

	.legend-node[data-shape='diamond'] {
		transform: rotate(45deg) scale(0.82);
		background: var(--white);
	}

	.legend-node.external {
		border-style: dashed;
		border-color: var(--gray-500);
		background: var(--gray-100);
	}

	.legend-node[data-shape='diamond'].external {
		background: var(--gray-100);
	}

	.legend-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}
</style>
