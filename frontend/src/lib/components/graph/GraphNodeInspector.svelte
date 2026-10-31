<script lang="ts">
	/**
	 * Node inspector for the graph explorer: selected node metadata, action
	 * buttons, the loaded edge list and the structural relation query.
	 *
	 * Structural results are page-local: they never join the canvas working
	 * set. A new selection invalidates the previous structural answer. The
	 * component owns the structural query state via the shared StaleQuery
	 * helper and talks to the graph store directly.
	 */
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { graphStore } from '$lib/stores/graph.svelte';
	import { watchStore } from '$lib/stores/watch.svelte';
	import { StaleQuery } from '$lib/stores/stale-query.svelte';
	import {
		CONFIDENCE_META,
		NODE_KINDS,
		domainLabel,
		edgeConfidence,
		normalizeNodeKind,
		relationLabel,
	} from '$lib/utils/graph-domains';
	import { clampPositive } from '$lib/utils/graph-params';
	import type {
		GraphEdge,
		GraphNode,
		GraphStructuralResponse,
	} from '$lib/api/graph';

	// Backend-defined structural relation families.
	const STRUCTURAL_KINDS = [
		'trait_bound',
		'child_elements',
		'parent_element',
		'event_handlers',
		'handler_elements',
		'parameter_bindings',
		'template_references',
		'template_ref_owners',
	] as const;
	type StructuralKind = (typeof STRUCTURAL_KINDS)[number];

	interface Props {
		node: GraphNode | null;
		edges: GraphEdge[];
		/** Component (community) id per node id, from the graph store meta. */
		communities: Record<string, number>;
		onExpand: (nodeId: string) => void;
		onCenter: (nodeId: string) => void;
		onFocusEntity: (entityId: string) => void;
	}

	let { node, edges, communities, onExpand, onCenter, onFocusEntity }: Props =
		$props();

	let structuralKind = $state<StructuralKind>('child_elements');
	let structuralDirection = $state<'out' | 'in'>('out');
	let structuralLimit = $state(200);
	let structuralFor = $state<string | null>(null);
	let query = new StaleQuery<GraphStructuralResponse>(
		() => watchStore.status?.relation_epoch ?? null,
	);

	// A new selection invalidates the previous structural answer.
	$effect(() => {
		void node;
		query.reset();
		structuralFor = null;
	});

	async function runStructural() {
		if (node === null) return;
		const subject = node;
		const limit = clampPositive(structuralLimit, 200, 2000);
		const ok = await query.run(() =>
			graphStore.loadStructural({
				entityId: subject.id,
				kind: structuralKind,
				direction: structuralDirection,
				limit,
			}),
		);
		if (ok) {
			structuralFor = subject.id;
		}
	}

	function openEntity(entityId: string) {
		goto(resolve(`/entities/${encodeURIComponent(entityId)}`));
	}
</script>

<aside class="inspector" aria-label="Node inspector">
	{#if node}
		<h4 class="inspector-title">{node.label}</h4>
		<dl class="meta-list">
			<dt>Kind</dt>
			<dd>
				{node.kind}
				<span class="kind-note"
					>{NODE_KINDS[normalizeNodeKind(node.kind)].description}</span
				>
			</dd>
			<dt>Location</dt>
			<dd class="mono">
				{node.source_file || node.source_location
					? `${node.source_file || '—'}:${node.source_location || ''}`
					: '—'}
			</dd>
			<dt>Community</dt>
			<dd>{communities[node.id] ?? '—'}</dd>
		</dl>

		<div class="inspector-actions">
			<Button variant="secondary" size="xs" onclick={() => openEntity(node.id)}>
				Open entity
			</Button>
			<Button variant="secondary" size="xs" onclick={() => onExpand(node.id)}>
				Expand
			</Button>
			<Button variant="secondary" size="xs" onclick={() => onCenter(node.id)}>
				Center
			</Button>
		</div>

		<h5 class="subsection-title">Relations ({edges.length})</h5>
		<ul class="relation-list">
			{#each edges.slice(0, 40) as edge (edge.source + edge.target + edge.relation)}
				{@const outgoing = edge.source === node.id}
				{@const confidence = edgeConfidence(edge.confidence)}
				<li class="relation-item">
					<div class="relation-head">
						<Badge variant={outgoing ? 'active' : 'default'}>
							{outgoing ? 'OUT' : 'IN'}
						</Badge>
						<span class="relation-domain">{domainLabel(edge)}</span>
					</div>
					<p class="relation-name">{relationLabel(edge.relation)}</p>
					<p class="relation-target mono">
						{outgoing ? edge.target : edge.source}
					</p>
					<p
						class="relation-confidence"
						title={CONFIDENCE_META[confidence].description}
					>
						{CONFIDENCE_META[confidence].label}
					</p>
					{#if edge.cfg_condition}
						<p
							class="relation-guard mono"
							title="Conditional-compilation guard"
						>
							cfg({edge.cfg_condition})
						</p>
					{/if}
				</li>
			{/each}
			{#if edges.length === 0}
				<li class="relation-empty">No loaded relations for this node.</li>
			{/if}
		</ul>

		<h5 class="subsection-title">
			Structural relations
			{#if query.stale}
				<span class="stale-tag">stale</span>
			{/if}
		</h5>
		<div class="structural-controls">
			<label class="graph-inline-field">
				<span class="graph-inline-label">Kind</span>
				<select
					class="text-input"
					bind:value={structuralKind}
					aria-label="Structural relation family"
				>
					{#each STRUCTURAL_KINDS as kind (kind)}
						<option value={kind}>{kind}</option>
					{/each}
				</select>
			</label>
			<label class="graph-inline-field">
				<span class="graph-inline-label">Dir</span>
				<select
					class="text-input text-input--narrow"
					bind:value={structuralDirection}
					aria-label="Structural direction"
				>
					<option value="out">out</option>
					<option value="in">in</option>
				</select>
			</label>
			<label class="graph-inline-field">
				<span class="graph-inline-label">Limit</span>
				<input
					class="text-input text-input--narrow"
					type="number"
					min="1"
					max="2000"
					bind:value={structuralLimit}
					aria-label="Structural result limit"
				/>
			</label>
			<Button
				variant="secondary"
				size="xs"
				onclick={runStructural}
				disabled={query.loading}
			>
				{query.loading ? 'Querying…' : 'Query'}
			</Button>
		</div>
		{#if query.error}
			<InlineError message={query.error} />
		{:else if query.data !== null && structuralFor === node.id}
			{#if query.data.relations.length === 0}
				<p class="panel-empty">
					No {query.data.kind} relations for this entity ({query.data
						.direction}).
				</p>
			{:else}
				<p class="structural-hint">
					{query.data.relations.length} relation{query.data.relations.length ===
					1
						? ''
						: 's'}
					{#if query.data.truncated}
						of {query.data.total_relations ?? query.data.relations.length} — increase
						the limit for the full list.
					{/if}
				</p>
				<ul class="relation-list structural-list">
					{#each query.data.relations as relation (relation.entity_id + relation.relation)}
						<li class="relation-item">
							<p class="relation-name">{relation.label}</p>
							<p class="relation-target mono">{relation.entity_id}</p>
							<p class="relation-meta">
								{relation.relation} · {relation.domain}
							</p>
							<p class="relation-target mono">{relation.source_file}</p>
							<div class="peer-actions">
								<Button
									variant="ghost"
									size="xs"
									onclick={() => openEntity(relation.entity_id)}
								>
									Open
								</Button>
								<Button
									variant="ghost"
									size="xs"
									onclick={() => onFocusEntity(relation.entity_id)}
								>
									Focus
								</Button>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{:else}
			<p class="panel-empty">Pick a relation family and run the query.</p>
		{/if}
	{:else}
		<EmptyState
			message="No node selected"
			hint="Click a node in the canvas to inspect it"
		/>
	{/if}
</aside>

<style>
	.inspector {
		border: 1px solid var(--black);
		padding: 1rem;
		min-height: 200px;
	}

	.inspector-title {
		font-size: 1.05rem;
		margin: 0 0 0.75rem;
		word-break: break-word;
	}

	.meta-list {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.25rem 0.75rem;
		margin: 0 0 1rem;
	}

	.meta-list dt {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.meta-list dd {
		margin: 0;
		font-size: 0.78rem;
		color: var(--black);
		word-break: break-all;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.inspector-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--gray-200);
		margin-bottom: 0.75rem;
	}

	.subsection-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		font-weight: 400;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		margin: 0 0 0.5rem;
	}

	.relation-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		max-height: 420px;
		overflow-y: auto;
	}

	.relation-item {
		border-left: 2px solid var(--gray-200);
		padding-left: 0.5rem;
	}

	.relation-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.relation-domain {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.relation-name {
		margin: 0.15rem 0 0;
		font-size: 0.78rem;
		color: var(--black);
	}

	.relation-target {
		margin: 0;
		color: var(--gray-500);
		word-break: break-all;
	}

	.relation-confidence {
		margin: 0.15rem 0 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.relation-guard {
		margin: 0.15rem 0 0;
		font-size: 0.6rem;
		color: var(--gray-500);
		word-break: break-all;
	}

	.relation-meta {
		margin: 0.15rem 0 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		color: var(--gray-500);
	}

	.kind-note {
		display: block;
		color: var(--gray-500);
		font-size: 0.7rem;
	}

	.relation-empty {
		font-size: 0.78rem;
		color: var(--gray-500);
		font-style: italic;
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

	.structural-list {
		max-height: 300px;
	}

	.structural-controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		margin-bottom: 0.5rem;
	}

	.structural-controls .text-input {
		min-width: 0;
		height: 26px;
		font-size: 0.68rem;
	}

	.structural-hint {
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: 0 0 0.5rem;
	}

	.peer-actions {
		display: flex;
		gap: 0.35rem;
		margin-top: 0.3rem;
	}

	.panel-empty {
		font-size: 0.78rem;
		color: var(--gray-500);
		font-style: italic;
		margin: 0;
	}

	@media (max-width: 1200px) {
		.inspector {
			min-height: auto;
		}
	}
</style>
