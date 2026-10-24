<script lang="ts">
	/**
	 * Seed bar for the graph explorer: seed mode tabs, per-mode inputs,
	 * impact analysis inputs and the advanced server-side filter disclosure.
	 * The bar owns no query logic — it binds form state and reports intent
	 * through the on* callbacks; the page executes the loads.
	 */
	import type { GraphDirection } from '$lib/api/graph';
	import Icon from '$lib/components/ui/Icon.svelte';

	type SeedMode = 'focus' | 'overview' | 'path' | 'subgraph';
	type ExternalEdgesMode = 'default' | 'on' | 'off';

	interface Props {
		seedMode?: SeedMode;
		seedId?: string;
		pathStart?: string;
		pathEnd?: string;
		pathDepth?: number;
		subgraphIds?: string;
		overviewLimit?: number;
		egoDepth?: number;
		egoDirection?: GraphDirection;
		impactFile?: string;
		impactMode?: 'file' | 'entity';
		impactEntity?: string;
		impactScope?: 'entity' | 'file';
		impactDepth?: number;
		serverRelationTypes?: string;
		serverIncludeExternal?: ExternalEdgesMode;
		loading?: boolean;
		impactBusy?: boolean;
		moduleLoading?: boolean;
		showClear?: boolean;
		onRunSeed: () => void;
		onRunImpact: () => void;
		onRunModule: () => void;
		onClearImpact: () => void;
	}

	let {
		seedMode = $bindable<SeedMode>('focus'),
		seedId = $bindable(''),
		pathStart = $bindable(''),
		pathEnd = $bindable(''),
		pathDepth = $bindable(10),
		subgraphIds = $bindable(''),
		overviewLimit = $bindable(400),
		egoDepth = $bindable(2),
		egoDirection = $bindable<GraphDirection>('both'),
		impactFile = $bindable(''),
		impactMode = $bindable<'file' | 'entity'>('file'),
		impactEntity = $bindable(''),
		impactScope = $bindable<'entity' | 'file'>('entity'),
		impactDepth = $bindable(10),
		serverRelationTypes = $bindable(''),
		serverIncludeExternal = $bindable<ExternalEdgesMode>('default'),
		loading = false,
		impactBusy = false,
		moduleLoading = false,
		showClear = false,
		onRunSeed,
		onRunImpact,
		onRunModule,
		onClearImpact,
	}: Props = $props();

	function handleSeedKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			onRunSeed();
		}
	}

	function handleImpactKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			onRunImpact();
		}
	}
</script>

<div class="seed-bar">
	<span class="group-label">Load</span>
	<div class="seed-mode">
		<button
			type="button"
			class="mode-btn"
			class:active={seedMode === 'focus'}
			onclick={() => (seedMode = 'focus')}
		>
			Focus entity
		</button>
		<button
			type="button"
			class="mode-btn"
			class:active={seedMode === 'overview'}
			onclick={() => (seedMode = 'overview')}
		>
			Project overview
		</button>
		<button
			type="button"
			class="mode-btn"
			class:active={seedMode === 'path'}
			onclick={() => (seedMode = 'path')}
		>
			Find path
		</button>
		<button
			type="button"
			class="mode-btn"
			class:active={seedMode === 'subgraph'}
			onclick={() => (seedMode = 'subgraph')}
		>
			Subgraph
		</button>
	</div>

	{#if seedMode === 'focus'}
		<input
			class="seed-input"
			type="text"
			placeholder="Symbol name, path#name or id…"
			bind:value={seedId}
			onkeydown={handleSeedKeydown}
			aria-label="Entity id to focus on"
		/>
		<label class="inline-field">
			<span class="inline-label">Depth</span>
			<input
				class="seed-input narrow"
				type="number"
				min="1"
				max="4"
				bind:value={egoDepth}
				aria-label="Neighborhood depth"
			/>
		</label>
		<label class="inline-field">
			<span class="inline-label">Dir</span>
			<select
				class="seed-input narrow"
				bind:value={egoDirection}
				aria-label="Traversal direction"
			>
				<option value="both">both</option>
				<option value="out">out</option>
				<option value="in">in</option>
			</select>
		</label>
	{:else if seedMode === 'path'}
		<input
			class="seed-input"
			type="text"
			placeholder="Start entity id…"
			bind:value={pathStart}
			onkeydown={handleSeedKeydown}
			aria-label="Path start entity id"
		/>
		<span class="path-arrow" aria-hidden="true"><Icon name="arrow-right" /></span>
		<input
			class="seed-input"
			type="text"
			placeholder="End entity id…"
			bind:value={pathEnd}
			onkeydown={handleSeedKeydown}
			aria-label="Path end entity id"
		/>
		<label class="inline-field">
			<span class="inline-label">Depth</span>
			<input
				class="seed-input narrow"
				type="number"
				min="1"
				max="100"
				bind:value={pathDepth}
				aria-label="Path maximum search depth"
			/>
		</label>
	{:else if seedMode === 'subgraph'}
		<input
			class="seed-input wide-input"
			type="text"
			placeholder="Comma-separated entity ids…"
			bind:value={subgraphIds}
			onkeydown={handleSeedKeydown}
			aria-label="Entity ids for induced subgraph"
		/>
	{:else}
		<label class="inline-field">
			<span class="inline-label">Limit</span>
			<input
				class="seed-input narrow"
				type="number"
				min="1"
				max="10000"
				bind:value={overviewLimit}
				aria-label="Overview node limit"
			/>
		</label>
	{/if}

	<button
		type="button"
		class="primary-btn"
		onclick={onRunSeed}
		disabled={loading}
	>
		{loading ? 'Loading…' : 'Load'}
	</button>

	<div class="seed-spacer"></div>

	<span class="group-label">Impact</span>
	<div class="impact-mode" role="group" aria-label="Impact mode">
		<button
			type="button"
			class="mode-btn"
			class:active={impactMode === 'file'}
			onclick={() => (impactMode = 'file')}
		>
			File
		</button>
		<button
			type="button"
			class="mode-btn"
			class:active={impactMode === 'entity'}
			onclick={() => (impactMode = 'entity')}
		>
			Entity
		</button>
	</div>
	{#if impactMode === 'file'}
		<input
			class="seed-input file-input"
			type="text"
			placeholder="Impact: file path…"
			bind:value={impactFile}
			onkeydown={handleImpactKeydown}
			aria-label="File path for impact analysis"
		/>
	{:else}
		<input
			class="seed-input file-input"
			type="text"
			placeholder="Impact: entity id…"
			bind:value={impactEntity}
			onkeydown={handleImpactKeydown}
			aria-label="Entity id for impact analysis"
		/>
		<label class="inline-field">
			<span class="inline-label">Scope</span>
			<select
				class="seed-input narrow"
				bind:value={impactScope}
				aria-label="Entity impact scope"
			>
				<option value="entity">entity</option>
				<option value="file">file</option>
			</select>
		</label>
		<label class="inline-field">
			<span class="inline-label">Depth</span>
			<input
				class="seed-input narrow"
				type="number"
				min="1"
				max="50"
				bind:value={impactDepth}
				aria-label="Entity impact depth"
			/>
		</label>
	{/if}
	<button
		type="button"
		class="ghost-btn"
		onclick={onRunImpact}
		disabled={impactBusy}
	>
		{impactBusy ? 'Analyzing…' : 'Impact'}
	</button>
	{#if impactMode === 'file'}
		<button
			type="button"
			class="ghost-btn"
			onclick={onRunModule}
			disabled={moduleLoading}
		>
			{moduleLoading ? 'Querying…' : 'Module'}
		</button>
	{/if}
	{#if showClear}
		<button type="button" class="ghost-btn" onclick={onClearImpact}
			>Clear</button
		>
	{/if}
</div>

<details class="adv-filter" aria-label="Server-side filters">
	<summary>Advanced server filters</summary>
	<div class="adv-filter-body">
		<label class="inline-field">
			<span class="inline-label">Relation types</span>
			<input
				class="seed-input wide-input"
				type="text"
				placeholder="call.direct, inheritance, … (empty = all)"
				bind:value={serverRelationTypes}
				aria-label="Server-side relation type filter"
			/>
		</label>
		<label class="inline-field">
			<span class="inline-label">External edges</span>
			<select
				class="seed-input narrow"
				bind:value={serverIncludeExternal}
				aria-label="Include external edges on the server"
			>
				<option value="default">default</option>
				<option value="on">on</option>
				<option value="off">off</option>
			</select>
		</label>
		<span class="adv-filter-hint">
			Applies on the next Load and replaces the working set. Domain toggles stay
			client-side only.
		</span>
	</div>
</details>

<style>
	.group-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		padding: 0 0.25rem;
		align-self: center;
		border-right: 1px solid var(--gray-300);
		padding-right: 0.5rem;
		margin-right: 0.25rem;
	}

	.seed-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.75rem;
		border: 1px solid var(--black);
		margin-bottom: 1rem;
	}

	.seed-mode {
		display: flex;
	}

	.mode-btn {
		padding: 0.4rem 0.75rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		border-right: none;
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	.mode-btn:last-child {
		border-right: 1px solid var(--gray-300);
	}

	.mode-btn.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
	}

	.seed-input {
		height: 30px;
		min-width: 220px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.seed-input:focus {
		outline: none;
		border-color: var(--black);
	}

	.seed-input.narrow {
		min-width: 70px;
		width: 70px;
	}

	.seed-input.wide-input {
		min-width: 320px;
	}

	.file-input {
		min-width: 200px;
	}

	.path-arrow {
		font-family: 'Space Mono', monospace;
		color: var(--gray-500);
	}

	.seed-spacer {
		flex: 1;
	}

	.adv-filter {
		border: 1px solid var(--gray-300);
		border-top: none;
		margin: -1rem 0 1rem;
		padding: 0.4rem 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}

	.adv-filter summary {
		cursor: pointer;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.adv-filter-body {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		padding-top: 0.5rem;
	}

	.adv-filter-hint {
		font-size: 0.62rem;
		color: var(--gray-500);
	}

	.inline-field {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.inline-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.primary-btn,
	.ghost-btn {
		height: 30px;
		padding: 0 0.85rem;
		border: 1px solid var(--black);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: all 0.15s;
	}

	.primary-btn {
		background: var(--black);
		color: var(--white);
	}

	.primary-btn:disabled {
		opacity: 0.5;
		cursor: wait;
	}

	.ghost-btn {
		background: var(--white);
		color: var(--black);
	}

	.ghost-btn:hover {
		background: var(--gray-100);
	}

	.impact-mode {
		display: flex;
	}

	.impact-mode .mode-btn {
		padding: 0.4rem 0.6rem;
	}
</style>
