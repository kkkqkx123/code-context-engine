<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { searchApi, type EntitySearchResultItem } from '$lib/api/search';
	import { currentProjectId } from '$lib/stores/project';
	import { errorMessage } from '$lib/utils/errors';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	let query = $state('');
	let kindFilter = $state('');
	let limit = $state(50);
	let results = $state<EntitySearchResultItem[]>([]);
	let total = $state(0);
	let elapsedMs = $state(0);
	let loading = $state(false);
	let searched = $state(false);
	let error = $state<string | null>(null);

	const KIND_OPTIONS = [
		'function',
		'method',
		'class',
		'struct',
		'interface',
		'trait',
		'impl',
	];

	async function runSearch() {
		const trimmed = query.trim();
		if (!trimmed) {
			error = 'Enter a search term first';
			return;
		}
		loading = true;
		error = null;
		try {
			const response = await searchApi.entitySearch({
				query: trimmed,
				project_id: $currentProjectId,
				kind_filter: kindFilter || undefined,
				limit,
			});
			results = response.items;
			total = response.total;
			elapsedMs = response.elapsed_ms;
			searched = true;
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
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

<svelte:head>
	<title>Entities - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Entity Explorer"
			subtitle="Full-text search across indexed functions, classes and other entities"
		/>

		<Card title="Search Entities" subtitle="FTS5 query over the entity index">
			<div class="search-bar">
				<input
					class="search-input"
					type="text"
					bind:value={query}
					placeholder="e.g. authenticate OR parse_config"
					aria-label="Entity search query"
					onkeydown={(e) => {
						if (e.key === 'Enter') runSearch();
					}}
				/>
				<select
					class="kind-select"
					bind:value={kindFilter}
					aria-label="Kind filter"
				>
					<option value="">All kinds</option>
					{#each KIND_OPTIONS as kind (kind)}
						<option value={kind}>{kind}</option>
					{/each}
				</select>
				<select
					class="kind-select"
					bind:value={limit}
					aria-label="Result limit"
				>
					<option value={20}>20</option>
					<option value={50}>50</option>
					<option value={100}>100</option>
				</select>
				<Button variant="secondary" onclick={runSearch} disabled={loading}>
					{loading ? 'Searching...' : 'Search'}
				</Button>
			</div>

			{#if error}
				<div class="inline-error">{error}</div>
			{:else if searched && results.length === 0}
				<p class="placeholder-text">No entities matched the query.</p>
			{:else if results.length > 0}
				<div class="table-wrap">
					<table>
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
				<p class="placeholder-text">
					Search the entity index by name. Results link to entity details with
					call chains and relationships.
				</p>
			{/if}
		</Card>
	</div>
</div>

<style>
	.search-bar {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: center;
	}

	.search-input {
		flex: 1;
		min-width: 220px;
		padding: 0.55rem 0.75rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		color: var(--black);
	}

	.search-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.kind-select {
		padding: 0.55rem 0.6rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		color: var(--black);
	}

	.kind-select:focus {
		outline: none;
		border-color: var(--accent);
	}

	.table-wrap {
		overflow-x: auto;
		border: 1px solid var(--gray-200);
		margin-top: 1.25rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}

	th {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		text-align: left;
		padding: 0.6rem 0.75rem;
		border-bottom: 1px solid var(--gray-200);
		background: var(--gray-50);
		white-space: nowrap;
	}

	td {
		padding: 0.55rem 0.75rem;
		border-bottom: 1px solid var(--gray-100);
	}

	tr:last-child td {
		border-bottom: none;
	}

	.entity-link {
		color: var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		font-weight: 700;
		text-decoration: none;
		border-bottom: 1px solid var(--accent);
	}

	.entity-link:hover {
		color: var(--accent);
	}

	.cell-signature {
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		color: var(--gray-600);
		max-width: 380px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.cell-mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		color: var(--gray-600);
		white-space: nowrap;
	}

	.result-meta {
		margin-top: 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-500);
	}

	.inline-error {
		margin-top: 1rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		padding: 1.5rem 0;
		text-align: center;
	}
</style>
