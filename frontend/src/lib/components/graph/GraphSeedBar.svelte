<script lang="ts">
	/**
	 * Seed bar for the graph explorer.
	 *
	 * The bar is split into three semantic sections so the three different
	 * intents it serves stay visually and cognitively separate:
	 *
	 *   - Load: replace the working set with a seeded view.
	 *   - Impact: highlight dependents of a changed file or entity.
	 *   - Module: inspect module-level relations of one file.
	 *
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

	function handleModuleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			onRunModule();
		}
	}
</script>

<div class="seed-bar">
	<section class="seed-section" aria-label="Load a graph view">
		<h3 class="seed-section-title">Load</h3>
		<div class="seed-section-body">
			<div class="seed-mode" role="group" aria-label="Seed mode">
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
					class="graph-seed-input"
					type="text"
					placeholder="Symbol name, path#name or id…"
					bind:value={seedId}
					onkeydown={handleSeedKeydown}
					aria-label="Entity id to focus on"
				/>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Depth</span>
					<input
						class="graph-seed-input narrow"
						type="number"
						min="1"
						max="4"
						bind:value={egoDepth}
						aria-label="Neighborhood depth"
					/>
				</label>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Dir</span>
					<select
						class="graph-seed-input narrow"
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
					class="graph-seed-input"
					type="text"
					placeholder="Start entity id…"
					bind:value={pathStart}
					onkeydown={handleSeedKeydown}
					aria-label="Path start entity id"
				/>
				<span class="path-arrow" aria-hidden="true"
					><Icon name="arrow-right" /></span
				>
				<input
					class="graph-seed-input"
					type="text"
					placeholder="End entity id…"
					bind:value={pathEnd}
					onkeydown={handleSeedKeydown}
					aria-label="Path end entity id"
				/>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Depth</span>
					<input
						class="graph-seed-input narrow"
						type="number"
						min="1"
						max="100"
						bind:value={pathDepth}
						aria-label="Path maximum search depth"
					/>
				</label>
			{:else if seedMode === 'subgraph'}
				<input
					class="graph-seed-input wide-input"
					type="text"
					placeholder="Comma-separated entity ids…"
					bind:value={subgraphIds}
					onkeydown={handleSeedKeydown}
					aria-label="Entity ids for induced subgraph"
				/>
			{:else}
				<label class="graph-inline-field">
					<span class="graph-inline-label">Limit</span>
					<input
						class="graph-seed-input narrow"
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
				title="Replace the working set with the seeded view"
			>
				{loading ? 'Loading…' : 'Load'}
			</button>
		</div>
	</section>

	<section class="seed-section" aria-label="Impact analysis">
		<h3 class="seed-section-title">Impact</h3>
		<div class="seed-section-body">
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
					class="graph-seed-input file-input"
					type="text"
					placeholder="Impact: file path…"
					bind:value={impactFile}
					onkeydown={handleImpactKeydown}
					aria-label="File path for impact analysis"
				/>
			{:else}
				<input
					class="graph-seed-input file-input"
					type="text"
					placeholder="Impact: entity id…"
					bind:value={impactEntity}
					onkeydown={handleImpactKeydown}
					aria-label="Entity id for impact analysis"
				/>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Scope</span>
					<select
						class="graph-seed-input narrow"
						bind:value={impactScope}
						aria-label="Entity impact scope"
					>
						<option value="entity">entity</option>
						<option value="file">file</option>
					</select>
				</label>
				<label class="graph-inline-field">
					<span class="graph-inline-label">Depth</span>
					<input
						class="graph-seed-input narrow"
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
				class="graph-ghost-btn"
				onclick={onRunImpact}
				disabled={impactBusy}
				title="Highlight direct and indirect dependents"
			>
				{impactBusy ? 'Analyzing…' : 'Impact'}
			</button>
			{#if showClear}
				<button
					type="button"
					class="graph-ghost-btn"
					onclick={onClearImpact}
					title="Clear impact highlighting"
				>
					Clear
				</button>
			{/if}
		</div>
	</section>

	<section class="seed-section" aria-label="Module relations">
		<h3 class="seed-section-title">Module</h3>
		<div class="seed-section-body">
			<input
				class="graph-seed-input file-input"
				type="text"
				placeholder="Module: file path…"
				bind:value={impactFile}
				onkeydown={handleModuleKeydown}
				aria-label="File path for module relations"
			/>
			<button
				type="button"
				class="graph-ghost-btn"
				onclick={onRunModule}
				disabled={moduleLoading}
				title="Query exports, callers and imports for this file"
			>
				{moduleLoading ? 'Querying…' : 'Module'}
			</button>
		</div>
	</section>
</div>

<details class="adv-filter" aria-label="Server-side filters">
	<summary>Advanced server filters</summary>
	<div class="adv-filter-body">
		<label class="graph-inline-field">
			<span class="graph-inline-label">Relation types</span>
			<input
				class="graph-seed-input wide-input"
				type="text"
				placeholder="call.direct, inheritance, … (empty = all)"
				bind:value={serverRelationTypes}
				aria-label="Server-side relation type filter"
			/>
		</label>
		<label class="graph-inline-field">
			<span class="graph-inline-label">External edges</span>
			<select
				class="graph-seed-input narrow"
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
	.seed-bar {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.seed-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.75rem;
		border: 1px solid var(--black);
		background: var(--white);
	}

	.seed-section-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin: 0;
		padding-bottom: 0.35rem;
		border-bottom: 1px solid var(--gray-200);
	}

	.seed-section-body {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.seed-mode,
	.impact-mode {
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

	.graph-seed-input {
		height: 30px;
		min-width: 220px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.graph-seed-input:focus {
		outline: none;
		border-color: var(--black);
	}

	.graph-seed-input.narrow {
		min-width: 70px;
		width: 70px;
	}

	.graph-seed-input.wide-input {
		min-width: 320px;
	}

	.file-input {
		min-width: 200px;
	}

	.path-arrow {
		font-family: 'Space Mono', monospace;
		color: var(--gray-500);
	}

	.adv-filter {
		border: 1px solid var(--gray-300);
		border-top: none;
		margin: -0.25rem 0 1rem;
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

	.graph-inline-field {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.graph-inline-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.primary-btn {
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


	.impact-mode .mode-btn {
		padding: 0.4rem 0.6rem;
	}

	@media (max-width: 900px) {
		.seed-bar {
			grid-template-columns: 1fr;
		}
	}
</style>
