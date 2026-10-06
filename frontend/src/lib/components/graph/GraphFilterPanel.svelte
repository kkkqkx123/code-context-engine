<script lang="ts">
	/**
	 * Relation domain and node search controls.
	 *
	 * Domain toggles drive edge visibility and the search box highlights
	 * matching nodes while dimming the rest.
	 */
	import {
		RELATION_DOMAIN_ORDER,
		RELATION_DOMAINS,
		relationLineStyle,
		type RelationDomain,
	} from '$lib/utils/graph-style';

	interface Props {
		domains?: RelationDomain[];
		/** Domains actually present in the current graph, used to disable empty toggles. */
		availableDomains?: RelationDomain[];
		search?: string;
		showEdgeLabels?: boolean;
		onToggleDomain?: (domain: RelationDomain) => void;
		onSearch?: (value: string) => void;
		onToggleEdgeLabels?: () => void;
	}

	let {
		domains = [],
		availableDomains = RELATION_DOMAIN_ORDER,
		search = $bindable(''),
		showEdgeLabels = $bindable(false),
		onToggleDomain = () => {},
		onSearch = () => {},
		onToggleEdgeLabels = () => {},
	}: Props = $props();

	let available = $derived(new Set(availableDomains));

	function handleSearchInput(event: Event) {
		const value = (event.currentTarget as HTMLInputElement).value;
		search = value;
		onSearch(value);
	}
</script>

<aside class="filter-panel" aria-label="Graph filters">
	<div class="panel-section">
		<h4 class="panel-title">Find</h4>
		<input
			class="search-input"
			type="search"
			placeholder="Node name…"
			value={search}
			oninput={handleSearchInput}
			aria-label="Highlight nodes by name"
		/>
	</div>

	<div class="panel-section">
		<h4 class="panel-title">Relation domains</h4>
		<ul class="domain-list">
			{#each RELATION_DOMAIN_ORDER as domain (domain)}
				{@const meta = RELATION_DOMAINS[domain]}
				{@const enabled = domains.includes(domain)}
				{@const present = available.has(domain)}
				<li>
					<button
						type="button"
						class="domain-toggle"
						class:active={enabled}
						class:absent={!present}
						disabled={!present}
						onclick={() => onToggleDomain(domain)}
						aria-pressed={enabled}
						title={present
							? meta.description
							: `${meta.description} (not present in this graph)`}
					>
						<span
							class="swatch"
							style="--swatch: {meta.color}; --line-style: {relationLineStyle(
								domain,
							)}"
						></span>
						<span class="domain-name">{meta.label}</span>
						<span class="domain-state">{enabled ? 'ON' : 'OFF'}</span>
					</button>
				</li>
			{/each}
		</ul>
	</div>

	<div class="panel-section">
		<h4 class="panel-title">Labels</h4>
		<label class="checkbox-row">
			<input
				type="checkbox"
				checked={showEdgeLabels}
				onchange={() => onToggleEdgeLabels()}
			/>
			<span>Show edge labels</span>
		</label>
		<p class="hint">
			Render the relation type on each edge. Disable on large graphs to reduce
			clutter.
		</p>
	</div>

	<div class="panel-section">
		<h4 class="panel-title">Reading the graph</h4>
		<p class="hint">
			Edge width is how much the code leans on the link: a strong relation type
			scaled up by how many call sites reach the target. A thick edge is one the
			surrounding code depends on from many places.
		</p>
		<p class="hint">
			Faint edges were deduced during resolution rather than named directly in
			the source. Edges pointing outside the project are drawn faded too.
			Guarded edges combine both attenuations and render faintest.
		</p>
		<p class="hint">
			Faded flat edges sit behind a conditional-compilation guard, so they only
			exist under that predicate. Dashed node outlines mark symbols resolved
			outside the project; transitive impact uses a solid outline so the two
			are never confused.
		</p>
	</div>
</aside>

<style>
	.filter-panel {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		padding: 1rem;
		border: 1px solid var(--black);
		background: var(--white);
		min-width: 210px;
	}

	.panel-section {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.panel-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		font-weight: 400;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		margin: 0;
	}

	.search-input {
		width: 100%;
		height: 30px;
		padding: 0 0.5rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.search-input:focus {
		outline: none;
		border-color: var(--black);
	}

	.domain-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.domain-toggle {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.35rem 0.5rem;
		background: none;
		border: 1px solid transparent;
		cursor: pointer;
		text-align: left;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-500);
		transition: all 0.15s;
	}

	.domain-toggle:hover:not(:disabled) {
		border-color: var(--gray-300);
	}

	.domain-toggle.active {
		color: var(--black);
	}

	.domain-toggle.absent {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.domain-state {
		font-size: 0.6rem;
		letter-spacing: 0.1em;
	}

	.swatch {
		display: block;
		width: 18px;
		height: 0;
		border-top-width: 3px;
		border-top-color: var(--swatch);
		border-top-style: var(--line-style, solid);
	}

	.checkbox-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
		cursor: pointer;
	}

	.hint {
		font-size: 0.7rem;
		color: var(--gray-400);
		margin: 0;
		line-height: 1.4;
	}
</style>
