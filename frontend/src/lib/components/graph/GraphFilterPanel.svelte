<script lang="ts">
	/**
	 * Relation domain and node search controls.
	 *
	 * Domain toggles drive edge visibility, the search box highlights matching
	 * nodes and dims the rest, and the ambiguous toggle hides relations the
	 * extraction pipeline flagged as uncertain.
	 */
	import {
		CONFIDENCE_META,
		RELATION_DOMAINS,
		relationLineStyle,
		type RelationDomain
	} from '$lib/utils/graph-style';

	interface Props {
		domains?: RelationDomain[];
		/** Domains actually present in the current graph, used to disable empty toggles. */
		availableDomains?: RelationDomain[];
		search?: string;
		hideAmbiguous?: boolean;
		onToggleDomain?: (domain: RelationDomain) => void;
		onSearch?: (value: string) => void;
		onToggleAmbiguous?: () => void;
	}

	let {
		domains = [],
		availableDomains = ['call', 'dependency', 'structural', 'reference', 'other'],
		search = $bindable(''),
		hideAmbiguous = $bindable(false),
		onToggleDomain = () => {},
		onSearch = () => {},
		onToggleAmbiguous = () => {}
	}: Props = $props();

	let available = $derived(new Set(availableDomains));
	const domainOrder: RelationDomain[] = ['call', 'dependency', 'structural', 'reference', 'other'];

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
			{#each domainOrder as domain (domain)}
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
						title={present ? meta.description : `${meta.description} (not present in this graph)`}
					>
						<span
							class="swatch"
							style="--swatch: {meta.color}; --dash: {relationLineStyle(domain) === 'solid' ? '0' : '3 2'}"
						></span>
						<span class="domain-name">{meta.label}</span>
						<span class="domain-state">{enabled ? 'ON' : 'OFF'}</span>
					</button>
				</li>
			{/each}
		</ul>
	</div>

	<div class="panel-section">
		<h4 class="panel-title">Confidence</h4>
		<label class="checkbox-row">
			<input type="checkbox" checked={hideAmbiguous} onchange={() => onToggleAmbiguous()} />
			<span>Hide ambiguous relations</span>
		</label>
		<p class="hint">{CONFIDENCE_META.ambiguous.description}</p>
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
		border-top: 3px var(--dash, 0) var(--swatch);
		border-top-style: solid;
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
