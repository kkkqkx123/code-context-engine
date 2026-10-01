<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { metricsApi, type AggregatedMetric } from '$lib/api/metrics';
	import { currentProjectId } from '$lib/stores/project';

	// ─── History query state ──────────────────────────────────────
	function toLocalInput(date: Date): string {
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
	}

	let historyFrom = $state(toLocalInput(new Date(Date.now() - 3600_000)));
	let historyTo = $state(toLocalInput(new Date()));
	let metricFilter = $state('');
	let scopeProject = $state(true);
	let historyRows = $state<AggregatedMetric[]>([]);
	let historyLoading = $state(false);
	let historyError = $state<string | null>(null);
	let historyLoaded = $state(false);

	async function loadHistory() {
		historyLoading = true;
		historyError = null;
		try {
			historyRows = await metricsApi.getHistory({
				from: new Date(historyFrom).toISOString(),
				to: new Date(historyTo).toISOString(),
				metric: metricFilter.trim() || undefined,
				project_id: scopeProject ? get(currentProjectId) : undefined
			});
			historyLoaded = true;
		} catch (e: any) {
			historyError = e?.message ?? 'Failed to load metric history';
		} finally {
			historyLoading = false;
		}
	}

	function formatNumber(value: number | null | undefined): string {
		if (value === null || value === undefined) return '-';
		return Number(value).toFixed(2);
	}

	function typeBadgeVariant(metricType: string): 'info' | 'active' | 'warning' {
		if (metricType === 'counter') return 'info';
		if (metricType === 'gauge') return 'active';
		return 'warning';
	}

	// ─── Prometheus export state ─────────────────────────────────
	let prometheusText = $state<string | null>(null);
	let prometheusLoading = $state(false);
	let prometheusError = $state<string | null>(null);

	async function loadPrometheus() {
		prometheusLoading = true;
		prometheusError = null;
		try {
			prometheusText = await metricsApi.getPrometheusMetrics();
		} catch (e: any) {
			prometheusError = e?.message ?? 'Failed to load Prometheus metrics';
		} finally {
			prometheusLoading = false;
		}
	}

	async function copyPrometheus() {
		if (!prometheusText) return;
		try {
			await navigator.clipboard.writeText(prometheusText);
		} catch {
			// Clipboard access may be unavailable; the text stays visible so the
			// user can copy it manually.
		}
	}

	// ─── Cleanup state ───────────────────────────────────────────
	let cleanupBefore = $state('');
	let cleanupAll = $state(false);
	let cleanupConfirmOpen = $state(false);
	let cleanupBusy = $state(false);
	let cleanupMessage = $state<string | null>(null);
	let cleanupError = $state<string | null>(null);

	async function runCleanup() {
		cleanupBusy = true;
		cleanupError = null;
		try {
			const response = await metricsApi.cleanup({
				all: cleanupAll || undefined,
				before: !cleanupAll && cleanupBefore ? new Date(cleanupBefore).toISOString() : undefined
			});
			cleanupMessage = `Deleted ${response.deleted_count} metric records`;
			cleanupConfirmOpen = false;
			historyLoaded = false;
			historyRows = [];
		} catch (e: any) {
			cleanupError = e?.message ?? 'Failed to clean up metrics';
		} finally {
			cleanupBusy = false;
		}
	}

	onMount(() => {
		loadHistory();
	});
</script>

<svelte:head>
	<title>Metrics - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader title="Metrics" subtitle="Historical aggregates, Prometheus export and retention" />

		<Card title="Metric History" subtitle="Aggregated metric windows over a time range">
			<div class="filter-bar">
				<label class="filter-field">
					<span class="filter-label">From</span>
					<input class="filter-input" type="datetime-local" bind:value={historyFrom} />
				</label>
				<label class="filter-field">
					<span class="filter-label">To</span>
					<input class="filter-input" type="datetime-local" bind:value={historyTo} />
				</label>
				<label class="filter-field">
					<span class="filter-label">Metric name</span>
					<input class="filter-input" type="text" bind:value={metricFilter} placeholder="e.g. index_duration" />
				</label>
				<label class="filter-check">
					<input type="checkbox" bind:checked={scopeProject} />
					<span>Current project only</span>
				</label>
				<Button variant="secondary" onclick={loadHistory} disabled={historyLoading}>
					{historyLoading ? 'Loading...' : 'Query'}
				</Button>
			</div>

			{#if historyError}
				<div class="inline-error">{historyError}</div>
			{:else if historyRows.length === 0}
				<p class="loading-text">
					{historyLoaded ? 'No metric records in the selected range' : 'Loading metric history...'}
				</p>
			{:else}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>Metric</th>
								<th>Type</th>
								<th>Operation</th>
								<th>Count</th>
								<th>Avg</th>
								<th>Median</th>
								<th>P90</th>
								<th>P99</th>
								<th>Max</th>
							</tr>
						</thead>
						<tbody>
							{#each historyRows as row (row.metric_name + row.timestamp + row.operation_type)}
								<tr>
									<td class="cell-name">{row.metric_name}</td>
									<td><Badge label={row.metric_type} variant={typeBadgeVariant(row.metric_type)} /></td>
									<td class="cell-mono">{row.operation_type ?? '-'}</td>
									<td>{row.count}</td>
									<td>{formatNumber(row.avg)}</td>
									<td>{formatNumber(row.median)}</td>
									<td>{formatNumber(row.p90)}</td>
									<td>{formatNumber(row.p99)}</td>
									<td>{formatNumber(row.max)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<div class="result-count">{historyRows.length} aggregated rows</div>
			{/if}
		</Card>

		<Card title="Prometheus Export" subtitle="Text exposition format for scraping">
			<div class="prom-actions">
				<Button variant="secondary" onclick={loadPrometheus} disabled={prometheusLoading}>
					{prometheusLoading ? 'Loading...' : 'Load Metrics'}
				</Button>
				{#if prometheusText}
					<Button variant="secondary" onclick={copyPrometheus}>Copy</Button>
				{/if}
			</div>
			{#if prometheusError}
				<div class="inline-error">{prometheusError}</div>
			{:else if prometheusText}
				<pre class="prom-output">{prometheusText}</pre>
			{:else}
				<p class="loading-text">Load the current Prometheus payload to inspect or copy it.</p>
			{/if}
		</Card>

		<Card title="Retention Cleanup" subtitle="Delete historical metric records">
			<p class="cleanup-warning">
				Deleted metric history cannot be recovered. Live system metrics are unaffected.
			</p>
			<div class="cleanup-form">
				<label class="filter-check">
					<input type="checkbox" bind:checked={cleanupAll} />
					<span>Delete all records</span>
				</label>
				<label class="filter-field">
					<span class="filter-label">Delete records before</span>
					<input
						class="filter-input"
						type="datetime-local"
						bind:value={cleanupBefore}
						disabled={cleanupAll}
					/>
				</label>
				<Button
					variant="danger"
					onclick={() => (cleanupConfirmOpen = true)}
					disabled={cleanupBusy || (!cleanupAll && !cleanupBefore)}
				>
					{cleanupBusy ? 'Cleaning...' : 'Clean Up'}
				</Button>
			</div>
			{#if cleanupMessage}
				<div class="inline-success">{cleanupMessage}</div>
			{/if}
			{#if cleanupError}
				<div class="inline-error">{cleanupError}</div>
			{/if}
		</Card>
	</div>
</div>

{#if cleanupConfirmOpen}
	<div
		class="dialog-overlay"
		onclick={() => (cleanupConfirmOpen = false)}
		onkeydown={(e) => {
			if (e.key === 'Escape') cleanupConfirmOpen = false;
		}}
		role="button"
		tabindex="0"
		aria-label="Close dialog"
	>
		<div
			class="dialog"
			onclick={(e) => e.stopPropagation()}
			role="dialog"
			aria-modal="true"
			aria-labelledby="cleanup-dialog-title"
			tabindex="-1"
			onkeydown={(e) => {
				if (e.key === 'Escape') cleanupConfirmOpen = false;
			}}
		>
			<h2 id="cleanup-dialog-title">Confirm Metrics Cleanup</h2>
			<p class="dialog-warning">
				{cleanupAll
					? 'All historical metric records will be permanently deleted.'
					: `All metric records before ${cleanupBefore} will be permanently deleted.`}
			</p>
			<div class="dialog-actions">
				<Button variant="secondary" onclick={() => (cleanupConfirmOpen = false)}>Cancel</Button>
				<Button variant="danger" onclick={runCleanup} disabled={cleanupBusy}>
					{cleanupBusy ? 'Cleaning...' : 'Confirm Cleanup'}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.filter-bar {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 1rem;
		margin-bottom: 1.5rem;
	}

	.filter-field {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.filter-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.filter-input {
		padding: 0.45rem 0.6rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		color: var(--black);
		min-width: 180px;
	}

	.filter-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.filter-check {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.85rem;
		color: var(--gray-600);
		padding-bottom: 0.45rem;
	}

	.table-wrap {
		overflow-x: auto;
		border: 1px solid var(--gray-200);
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
		white-space: nowrap;
	}

	tr:last-child td {
		border-bottom: none;
	}

	.cell-name {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.cell-mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.result-count {
		margin-top: 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-500);
	}

	.prom-actions {
		display: flex;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.prom-output {
		background: var(--gray-900);
		color: var(--gray-200);
		padding: 1rem;
		overflow: auto;
		max-height: 320px;
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		line-height: 1.5;
	}

	.cleanup-warning {
		color: var(--gray-600);
		margin-bottom: 1rem;
		line-height: 1.6;
	}

	.cleanup-form {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 1rem;
	}

	.inline-error {
		margin-top: 1rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.inline-success {
		margin-top: 1rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--success);
		color: var(--success);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.loading-text {
		color: var(--gray-400);
		font-style: italic;
		padding: 1rem 0;
	}

	.dialog-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.dialog {
		background: var(--white);
		border: 2px solid var(--danger);
		padding: 2rem;
		max-width: 480px;
		width: 90%;
	}

	.dialog h2 {
		margin-bottom: 1rem;
		color: var(--black);
	}

	.dialog-warning {
		color: var(--gray-600);
		margin-bottom: 2rem;
		line-height: 1.6;
	}

	.dialog-actions {
		display: flex;
		gap: 1rem;
		justify-content: flex-end;
	}
</style>
