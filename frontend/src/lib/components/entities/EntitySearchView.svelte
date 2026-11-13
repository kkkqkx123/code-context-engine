<script lang="ts">
	/**
	 * Entity full-text search view of the query workbench.
	 *
	 * Query, kind filter and result limit live in the address bar so a shared
	 * URL reproduces the same view. The view follows the globally selected
	 * project, like every other query surface.
	 */
	import { onMount } from 'svelte';
	import { SvelteURLSearchParams } from 'svelte/reactivity';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import SymbolCandidatePanel from '$lib/components/ui/SymbolCandidatePanel.svelte';
	import { searchApi, type EntitySearchResultItem } from '$lib/api/search';
	import { entityStore } from '$lib/stores/entities.svelte';
	import { projectStore, onProjectChange } from '$lib/stores/project.svelte';
	import { toAppError, type AppError } from '$lib/utils/errors';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	const KIND_OPTIONS = [
		'function',
		'method',
		'class',
		'struct',
		'interface',
		'trait',
		'impl',
	];
	const LIMIT_OPTIONS = [20, 50, 100];

	function urlParam(key: string): string {
		if (typeof window === 'undefined') return '';
		return new SvelteURLSearchParams(window.location.search).get(key) ?? '';
	}

	function initialLimit(): number {
		const parsed = Number(urlParam('limit'));
		return LIMIT_OPTIONS.includes(parsed) ? parsed : 50;
	}

	function initialKind(): string {
		const kind = urlParam('kind');
		return KIND_OPTIONS.includes(kind) ? kind : '';
	}

	let query = $state(urlParam('q'));
	let kindFilter = $state(initialKind());
	let limit = $state(initialLimit());
	let results = $state<EntitySearchResultItem[]>([]);
	let total = $state(0);
	let elapsedMs = $state(0);
	let loading = $state(false);
	let searched = $state(false);
	let validationError = $state<string | null>(null);
	let searchError = $state<AppError | null>(null);
	let classificationLimit = $state(50);
	let classificationBusy = $state(false);

	let candidates = $derived(searchError?.candidates ?? []);

	function syncUrl() {
		if (typeof window === 'undefined') return;
		const url = new URL(window.location.href);
		const trimmed = query.trim();
		if (trimmed) url.searchParams.set('q', trimmed);
		else url.searchParams.delete('q');
		if (kindFilter) url.searchParams.set('kind', kindFilter);
		else url.searchParams.delete('kind');
		url.searchParams.set('limit', String(limit));
		url.searchParams.set('view', 'entities');
		window.history.replaceState(null, '', url);
	}

	async function runSearch() {
		const trimmed = query.trim();
		if (!trimmed) {
			validationError = 'Enter a search term first';
			return;
		}
		validationError = null;
		loading = true;
		searchError = null;
		try {
			const response = await searchApi.entitySearch({
				query: trimmed,
				project_id: projectStore.currentProjectId,
				kind_filter: kindFilter || undefined,
				limit,
			});
			results = response.items;
			total = response.total;
			elapsedMs = response.elapsed_ms;
			searched = true;
			syncUrl();
		} catch (e) {
			searchError = toAppError(e, 'Entity search failed');
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		if (query.trim()) void runSearch();
	});

	// Listed entities belong to the project they were searched under.
	$effect(() =>
		onProjectChange(() => {
			results = [];
			searched = false;
			searchError = null;
			validationError = null;
			entityStore.clear();
			if (query.trim()) void runSearch();
		}),
	);

	async function runClassificationStats() {
		classificationBusy = true;
		try {
			await entityStore.loadClassificationStats();
		} finally {
			classificationBusy = false;
		}
	}

	async function runClassificationDetail(category: string) {
		classificationBusy = true;
		try {
			await entityStore.loadRelationsByClassification(
				category,
				classificationLimit,
			);
		} finally {
			classificationBusy = false;
		}
	}

	function kindBadgeVariant(
		kind: string,
	): 'info' | 'active' | 'warning' | 'default' {
		if (kind === 'function' || kind === 'method') return 'info';
		if (kind === 'class' || kind === 'struct') return 'active';
		if (kind === 'interface' || kind === 'trait') return 'warning';
		return 'default';
	}

	function truncateSignature(signature: string | null | undefined): string {
		if (!signature) return '-';
		const oneLine = signature.replace(/\s+/g, ' ').trim();
		return oneLine.length > 96 ? `${oneLine.slice(0, 96)}...` : oneLine;
	}
</script>

<Card
	title="Entity Search"
	subtitle={`FTS5 query over the entity index · shows up to ${limit} results, no paging`}
>
	<div class="tool-form-row">
		<input
			class="text-input"
			type="text"
			bind:value={query}
			placeholder="e.g. authenticate OR parse_config"
			aria-label="Entity search query"
			onkeydown={(e) => {
				if (e.key === 'Enter') runSearch();
			}}
		/>
		<select class="select" bind:value={kindFilter} aria-label="Kind filter">
			<option value="">All kinds</option>
			{#each KIND_OPTIONS as kind (kind)}
				<option value={kind}>{kind}</option>
			{/each}
		</select>
		<select class="select" bind:value={limit} aria-label="Result limit">
			{#each LIMIT_OPTIONS as option (option)}
				<option value={option}>{option}</option>
			{/each}
		</select>
		<Button variant="secondary" onclick={runSearch} disabled={loading}>
			{loading ? 'Searching...' : 'Search'}
		</Button>
	</div>

	{#if validationError}
		<InlineError message={validationError} />
	{/if}

	{#if searchError}
		<ErrorBanner message={searchError.message} code={searchError.code} />
		{#if candidates.length > 0}
			<SymbolCandidatePanel
				{candidates}
				onPick={(candidate) => {
					query = candidate.scoped_name;
					void runSearch();
				}}
			/>
		{/if}
	{:else if searched && results.length === 0}
		<p class="empty-hint">No entities matched the query.</p>
	{:else if results.length > 0}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Name</th>
						<th>Kind</th>
						<th>Signature</th>
						<th>File</th>
						<th>Lines</th>
					</tr>
				</thead>
				<tbody>
					{#each results as entity (entity.id)}
						<tr>
							<td>
								<a
									class="entity-link"
									href={resolve(`/entities/${entity.id}`)}
									onclick={(e) => {
										e.preventDefault();
										goto(resolve(`/entities/${entity.id}`));
									}}>{entity.name}</a
								>
							</td>
							<td
								><Badge
									label={entity.kind}
									variant={kindBadgeVariant(entity.kind)}
								/></td
							>
							<td class="cell-signature"
								>{truncateSignature(entity.signature)}</td
							>
							<td class="cell-mono">#{entity.file_id}</td>
							<td class="cell-mono">
								{#if entity.span_start_row !== null && entity.span_start_row !== undefined}
									{entity.span_start_row}{entity.span_end_row !== null &&
									entity.span_end_row !== undefined
										? `-${entity.span_end_row}`
										: ''}
								{:else}
									-
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<div class="result-meta">
			{results.length} of {total} matches · {elapsedMs} ms
		</div>
	{:else}
		<p class="empty-hint">
			Search the entity index by name. Results link to entity details with call
			chains and relationships.
		</p>
	{/if}
</Card>

<Card
	title="Classification"
	subtitle="External-call classification counts and category detail"
>
	<div class="tool-form-row">
		<label class="form-label" for="classification-limit">Detail limit</label>
		<select
			id="classification-limit"
			class="select"
			bind:value={classificationLimit}
			aria-label="Classification detail limit"
		>
			{#each LIMIT_OPTIONS as option (option)}
				<option value={option}>{option}</option>
			{/each}
		</select>
		<Button
			variant="secondary"
			onclick={runClassificationStats}
			disabled={classificationBusy}
		>
			{classificationBusy ? 'Loading...' : 'Load stats'}
		</Button>
	</div>

	{#if entityStore.error}
		<InlineError message={entityStore.error.message} />
	{:else if entityStore.classificationStats}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Category</th>
						<th>Count</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each Object.entries(entityStore.classificationStats.stats) as [category, count] (category)}
						<tr>
							<td class="cell-mono">{category}</td>
							<td class="cell-mono">{count}</td>
							<td>
								<Button
									variant="secondary"
									onclick={() => runClassificationDetail(category)}
									disabled={classificationBusy}
								>
									Detail
								</Button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<div class="result-meta">
			{entityStore.classificationStats.total} classified relations
			{#if entityStore.classificationCategory}
				· showing {entityStore.classificationRelations.length} in
				{entityStore.classificationCategory}
			{/if}
		</div>
	{:else}
		<p class="empty-hint">
			Load per-category counts, then drill into one category.
		</p>
	{/if}
</Card>

<style>
	.entity-link {
		color: var(--black);
		font-family: var(--font-mono);
		font-size: 0.8rem;
		font-weight: 700;
		text-decoration: none;
		border-bottom: 1px solid var(--accent);
	}

	.entity-link:hover {
		color: var(--accent);
	}

	.cell-signature {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-600);
		max-width: 380px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.cell-mono {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-600);
		white-space: nowrap;
	}

	.result-meta {
		margin-top: 0.75rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
	}
</style>
