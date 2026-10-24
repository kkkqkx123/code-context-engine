<script lang="ts">
	/**
	 * Dependency-cycle detection panel. Page-local results: they never join
	 * the canvas working set. Query state (loading / error / epoch / stale)
	 * is owned by the shared StaleQuery helper.
	 */
	import { graphActions } from '$lib/stores/graph';
	import { watchState } from '$lib/stores/watch';
	import { StaleQuery } from '$lib/stores/stale-query.svelte';
	import { clampPositive } from '$lib/utils/graph-params';

	interface Props {
		/** Called with a cycle's members to load them as an induced subgraph. */
		onLoadSubgraph: (members: string[]) => void;
	}

	let { onLoadSubgraph }: Props = $props();

	let cyclesLevel = $state<'entity' | 'file'>('entity');
	let cyclesLimit = $state(100);
	type GraphCyclesResponseData = import('$lib/api/graph').GraphCyclesResponse;
	let query = new StaleQuery<GraphCyclesResponseData>(
		() => $watchState.status?.relation_epoch ?? null,
	);

	let sortedCycles = $derived.by(() => {
		const data = query.data;
		if (data === null) return [];
		return [...data.cycles].sort((a, b) => b.members.length - a.members.length);
	});

	async function runCycles() {
		const limit = clampPositive(cyclesLimit, 100, 5000);
		await query.run(() => graphActions.loadCycles(cyclesLevel, limit));
	}
</script>

<section class="panel-section-block" aria-label="Dependency cycles">
	<div class="panel-head">
		<h4 class="communities-title">Dependency cycles</h4>
		{#if query.stale}
			<span class="stale-tag">Stale — index advanced to {$watchState.status?.relation_epoch}</span>
		{/if}
	</div>
	<div class="panel-controls">
		<label class="inline-field">
			<span class="inline-label">Level</span>
			<select class="seed-input narrow" bind:value={cyclesLevel} aria-label="Cycle detection level">
				<option value="entity">entity</option>
				<option value="file">file</option>
			</select>
		</label>
		<label class="inline-field">
			<span class="inline-label">Limit</span>
			<input
				class="seed-input narrow"
				type="number"
				min="1"
				max="5000"
				bind:value={cyclesLimit}
				aria-label="Cycle result limit"
			/>
		</label>
		<button type="button" class="ghost-btn" onclick={runCycles} disabled={query.loading}>
			{query.loading ? 'Detecting…' : 'Detect'}
		</button>
	</div>
	{#if query.error}
		<p class="panel-error">{query.error}</p>
	{:else if query.data !== null}
		{#if sortedCycles.length === 0}
			<p class="panel-empty">No dependency cycles detected at {query.data.level} level.</p>
		{:else}
			<p class="communities-hint">
				{query.data.cycles.length} cycle{query.data.cycles.length === 1 ? '' : 's'} at
				{query.data.level} level, largest first.
				{#if query.data.truncated}
					More cycles exist beyond the limit (detected ≥
					{query.data.total_cycles ?? query.data.cycles.length}).
				{/if}
			</p>
			<ul class="cycle-list">
				{#each sortedCycles as cycle, index (index)}
					<li class="cycle-item">
						<div class="cycle-head">
							<span class="community-index">#{index}</span>
							<span class="community-size">{cycle.members.length} members</span>
							{#if cyclesLevel === 'entity'}
								<button
									type="button"
									class="ghost-btn small-btn"
									onclick={() => onLoadSubgraph(cycle.members)}
								>
									Load as subgraph
								</button>
							{/if}
						</div>
						<p class="community-preview mono">
							{cycle.members.slice(0, 4).join(' → ')}{cycle.members.length > 4
								? ` … (+${cycle.members.length - 4})`
								: ''}
						</p>
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<p class="panel-empty">Not checked yet — pick a level and run detection.</p>
	{/if}
</section>

<style>
	.panel-section-block {
		margin-top: 1rem;
		padding: 1rem;
		border: 1px solid var(--black);
	}

	.panel-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.25rem;
	}

	.communities-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
		margin: 0 0 0.25rem;
	}

	.stale-tag {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--warning);
		border: 1px solid var(--warning);
		padding: 0.1rem 0.4rem;
	}

	.panel-controls {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		margin: 0.5rem 0 0.75rem;
	}

	.panel-error {
		font-size: 0.75rem;
		color: var(--danger);
		margin: 0;
	}

	.panel-empty {
		font-size: 0.78rem;
		color: var(--gray-500);
		font-style: italic;
		margin: 0;
	}

	.communities-hint {
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: 0 0 0.75rem;
	}

	.cycle-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
	}

	.cycle-item {
		border: 1px solid var(--gray-300);
		padding: 0.4rem 0.5rem;
	}

	.cycle-item .community-preview {
		margin: 0.3rem 0 0;
		white-space: normal;
		word-break: break-all;
	}

	.cycle-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.community-index {
		color: var(--black);
	}

	.community-size {
		color: var(--gray-500);
	}

	.community-preview {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.small-btn {
		height: 24px;
		padding: 0 0.6rem;
		margin-left: auto;
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

	.seed-input {
		height: 30px;
		min-width: 220px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.seed-input.narrow {
		min-width: 70px;
		width: 70px;
	}

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
		background: var(--white);
		color: var(--black);
	}

	.ghost-btn:hover {
		background: var(--gray-100);
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}
</style>
