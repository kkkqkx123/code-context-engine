<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { entityState, entityActions } from '$lib/stores/entities';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';

	// Lazy load all heavy components
	let EntityDetail: any = $state(null);
	let CallGraph: any = $state(null);
	let InheritanceTree: any = $state(null);
	
	let entityDetailLoaded = $state(false);
	let callGraphLoaded = $state(false);
	let inheritanceTreeLoaded = $state(false);

	let entityId = $derived(page.params.id);
	let currentTab = $state('details');

	onMount(async () => {
		if (entityId) {
			// Load entity data first
			await Promise.all([
				// Try loading as function first, fallback to class
				entityActions.loadFunction(entityId).then(() => {
					if (!$entityState.currentEntity) {
						return entityActions.loadClass(entityId);
					}
				})
			]);
			// EntityDetail will be loaded when details tab is activated
		}
	});

	async function loadEntityDetailComponent() {
		if (!entityDetailLoaded) {
			const module = await import('$lib/components/entities/EntityDetail.svelte');
			EntityDetail = module.default;
			entityDetailLoaded = true;
		}
	}

	function handleNavigate(id: string) {
		window.location.href = `/entities/${id}`;
	}

	function loadCallChain(direction: 'up' | 'down') {
		if (entityId) {
			entityActions.loadCallChain(entityId, direction);
		}
	}

	function handleChainKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			handleNavigate(String(id));
		}
	}

	async function loadCallGraphComponent() {
		if (!callGraphLoaded) {
			const module = await import('$lib/components/entities/CallGraph.svelte');
			CallGraph = module.default;
			callGraphLoaded = true;
		}
	}

	async function loadInheritanceTreeComponent() {
		if (!inheritanceTreeLoaded) {
			const module = await import('$lib/components/entities/InheritanceTree.svelte');
			InheritanceTree = module.default;
			inheritanceTreeLoaded = true;
		}
	}
</script>

<svelte:head>
	<title>Entity Details - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		{#if $entityState.isLoading}
			<div class="loading-state">
				<p>Loading entity...</p>
			</div>
		{:else if $entityState.error}
			<div class="error-state">
				<p>{$entityState.error}</p>
			</div>
		{:else if $entityState.currentEntity}
			<h1>Entity Details</h1>
			<p class="page-description">Viewing entity: {$entityState.currentEntity.name}</p>

			<!-- Tab Navigation -->
			<div class="tab-navigation">
				<button 
					class="tab" 
					class:active={currentTab === 'details'}
					onclick={() => {
						currentTab = 'details';
						loadEntityDetailComponent();
					}}
				>
					Details
				</button>
				<button 
					class="tab" 
					class:active={currentTab === 'call-graph'}
					onclick={() => {
						currentTab = 'call-graph';
						loadCallGraphComponent();
						loadCallChain('down');
					}}
				>
					Call Graph
				</button>
				<button 
					class="tab" 
					class:active={currentTab === 'inheritance'}
					onclick={() => {
						currentTab = 'inheritance';
						loadInheritanceTreeComponent();
					}}
				>
					Inheritance
				</button>
				<button 
					class="tab" 
					class:active={currentTab === 'call-chain'}
					onclick={() => {
						currentTab = 'call-chain';
						loadCallChain('down');
					}}
				>
					Call Chain
				</button>
			</div>

			<!-- Tab Content -->
			{#if currentTab === 'details'}
				{#if entityDetailLoaded && EntityDetail}
					<EntityDetail
						func={$entityState.currentEntity}
						calls={$entityState.calls?.callees ?? []}
						callers={$entityState.callers?.callers ?? []}
						onNavigate={handleNavigate}
					/>
				{:else}
					<button 
						class="loading-spinner-button" 
						onclick={loadEntityDetailComponent}
						aria-label="Load entity details"
					>
						Loading entity details... (click to load)
					</button>
				{/if}
			{:else if currentTab === 'call-graph'}
				<Card title="Call Graph" subtitle="Visual relationship map">
					{#if CallGraph}
						<CallGraph 
							nodes={$entityState.callChain}
							onNavigate={handleNavigate}
						/>
					{:else}
						<div class="loading-spinner">Loading graph...</div>
					{/if}
				</Card>
			{:else if currentTab === 'inheritance'}
				<Card title="Inheritance Tree" subtitle="Class hierarchy">
					{#if InheritanceTree}
						<InheritanceTree 
							inheritance={$entityState.inheritance}
							implementations={$entityState.implementations}
							onNavigate={handleNavigate}
						/>
					{:else}
						<div class="loading-spinner">Loading tree...</div>
					{/if}
				</Card>
			{:else if currentTab === 'call-chain'}
				<Card title="Call Chain" subtitle="Linear execution path">
					{#if $entityState.callChain.length > 0}
						<div class="call-chain-list">
							{#each $entityState.callChain as node, i}
								<div 
									class="chain-item"
									role="button"
									tabindex="0"
									onclick={() => handleNavigate(String(node.function_id))}
									onkeydown={(e) => handleChainKeydown(e, node.function_id)}
								>
									<span class="chain-number">{i + 1}</span>
									<div class="chain-content">
										<h4 class="chain-name">{node.function_name}</h4>
										<p class="chain-location">{node.file_path}{#if node.call_line}:{node.call_line}{/if}</p>
									</div>
									<Badge variant={node.relation_type === 'caller' ? 'default' : 'active'}>
										{node.relation_type === 'caller' ? 'CALLER' : 'CALLEE'}
									</Badge>
								</div>
							{/each}
						</div>
					{:else}
						<p class="placeholder-text">No call chain data available</p>
					{/if}
				</Card>
			{/if}
		{:else}
			<div class="not-found">
				<h2>Entity Not Found</h2>
				<p>No entity found with ID: {entityId}</p>
			</div>
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

	.loading-state,
	.error-state,
	.not-found {
		padding: 4rem 2rem;
		text-align: center;
		border: 1px solid var(--gray-200);
	}

	.loading-state p,
	.error-state p {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}

	.error-state {
		border-color: var(--danger);
		color: var(--danger);
	}

	.not-found h2 {
		font-size: 1.5rem;
		margin-bottom: 1rem;
	}

	.tab-navigation {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 0.5rem;
		margin-bottom: 2rem;
		border-bottom: 1px solid var(--gray-200);
		padding-bottom: 1rem;
	}

	.tab {
		padding: 0.75rem 1rem;
		background: var(--white);
		border: 1px solid var(--gray-200);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: all 0.2s;
	}

	.tab:hover {
		background: var(--gray-100);
	}

	.tab.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
	}

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

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 2rem;
	}

	.loading-spinner {
		padding: 2rem;
		text-align: center;
		color: var(--gray-600);
		font-family: 'Space Mono', monospace;
	}

	.loading-spinner-button {
		padding: 2rem;
		text-align: center;
		color: var(--gray-600);
		font-family: 'Space Mono', monospace;
		background: none;
		border: 1px dashed var(--gray-300);
		width: 100%;
		cursor: pointer;
		transition: all 0.2s ease;
	}

	.loading-spinner-button:hover {
		border-color: var(--accent);
		color: var(--black);
		background: var(--gray-50);
	}
</style>
