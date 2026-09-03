<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
</script>

<svelte:head>
	<title>Entities - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Entity Explorer</h1>
		<p class="page-description">Browse and explore code entities, relationships, and call chains</p>

		<Card title="Quick Navigation" subtitle="Search for entities to explore">
			<p>Use the search interface to find specific entities, or navigate directly if you know the entity ID.</p>
			<div style="margin-top: 1.5rem; display: flex; gap: 1rem;">
				<a href="/search" style="text-decoration: none;">
					<button class="nav-button">Go to Search</button>
				</a>
			</div>
		</Card>

		<Card title="Entity Types" subtitle="Supported entity exploration">
			<div class="entity-types">
				<div class="type-item">
					<h3>Functions</h3>
					<p>Explore function signatures, callers, callees, and call chains</p>
				</div>
				<div class="type-item">
					<h3>Classes</h3>
					<p>View inheritance hierarchies and implementations</p>
				</div>
				<div class="type-item">
					<h3>Relationships</h3>
					<p>Visualize call graphs and dependency trees</p>
				</div>
			</div>
		</Card>
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

	.nav-button {
		padding: 0.75rem 1.5rem;
		background: var(--black);
		color: var(--white);
		border: none;
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.75rem;
		letter-spacing: 0.05em;
		cursor: pointer;
		transition: all 0.2s;
	}

	.nav-button:hover {
		background: var(--accent);
		transform: translateX(4px);
	}

	.entity-types {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
		gap: 1.5rem;
		margin-top: 1rem;
	}

	.type-item h3 {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.1rem;
		font-weight: 700;
		margin-bottom: 0.5rem;
		letter-spacing: -0.03em;
	}

	.type-item p {
		font-size: 0.9rem;
		color: var(--gray-600);
		line-height: 1.5;
	}
</style>
