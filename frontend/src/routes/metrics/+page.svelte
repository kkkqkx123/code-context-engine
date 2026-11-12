<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import type { Tone } from '$lib/utils/tone';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import { onMount } from 'svelte';
	import { metricsApi, type AggregatedMetric } from '$lib/api/metrics';
	import { projectStore, onProjectChange } from '$lib/stores/project.svelte';

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
				project_id: scopeProject ? projectStore.currentProjectId : undefined,
			});
			historyLoaded = true;
		} catch (e) {
			historyError = errorMessage(e) ?? 'Failed to load metric history';
		} finally {
			historyLoading = false;
		}
	}

	// History scoped to the current project must be re-read after a switch.
	$effect(() =>
		onProjectChange(() => {
			if (scopeProject && historyLoaded) void loadHistory();
		}),
	);

	function formatNumber(value: number | null | undefined): string {
		if (value === null || value === undefined) return '-';
		return Number(value).toFixed(2);
	}

	function typeBadgeVariant(
		metricType: string,
	): Extract<Tone, 'info' | 'success' | 'warning'> {
		if (metricType === 'counter') return 'info';
		if (metricType === 'gauge') return 'success';
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
		} catch (e) {
			prometheusError = errorMessage(e) ?? 'Failed to load Prometheus metrics';
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
				before:
					!cleanupAll && cleanupBefore
						? new Date(cleanupBefore).toISOString()
						: undefined,
			});
			cleanupMessage = `Deleted ${response.deleted_count} metric records`;
			cleanupConfirmOpen = false;
			historyLoaded = false;
			historyRows = [];
		} catch (e) {
			cleanupError = errorMessage(e) ?? 'Failed to clean up metrics';
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
		<PageHeader
			title="Metrics"
			subtitle="Historical aggregates, Prometheus export and retention"
		/>

		<Card
			title="Metric History"
			subtitle="Aggregated metric windows over a time range"
		>
			<div class="tool-form-row">
				<label class="form-field">
					<span class="form-label">From</span>
					<input
						class="text-input"
						type="datetime-local"
						bind:value={historyFrom}
					/>
				</label>
				<label class="form-field">
					<span class="form-label">To</span>
					<input
						class="text-input"
						type="datetime-local"
						bind:value={historyTo}
					/>
				</label>
				<label class="form-field">
					<span class="form-label">Metric name</span>
					<input
						class="text-input"
						type="text"
						bind:value={metricFilter}
						placeholder="e.g. index_duration"
					/>
				</label>
				<label class="filter-check">
					<input type="checkbox" bind:checked={scopeProject} />
					<span>Current project only</span>
				</label>
				<Button
					variant="secondary"
					onclick={loadHistory}
					disabled={historyLoading}
				>
					{historyLoading ? 'Loading...' : 'Query'}
				</Button>
			</div>

			{#if historyError}
				<InlineError message={historyError} />
			{:else if historyRows.length === 0}
				<LoadingState
					message={historyLoaded
						? 'No metric records in the selected range'
						: 'Loading metric history...'}
				/>
			{:else}
				<div class="table-wrap">
					<table class="table">
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
									<td class="font-mono">{row.metric_name}</td>
									<td
										><Badge
											label={row.metric_type}
											variant={typeBadgeVariant(row.metric_type)}
										/></td
									>
									<td class="font-mono">{row.operation_type ?? '-'}</td>
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

		<Card
			title="Prometheus Export"
			subtitle="Text exposition format for scraping"
		>
			<div class="tool-form-row">
				<Button
					variant="secondary"
					onclick={loadPrometheus}
					disabled={prometheusLoading}
				>
					{prometheusLoading ? 'Loading...' : 'Load Metrics'}
				</Button>
				{#if prometheusText}
					<Button variant="secondary" onclick={copyPrometheus}>Copy</Button>
				{/if}
			</div>
			{#if prometheusError}
				<InlineError message={prometheusError} />
			{:else if prometheusText}
				<pre class="prom-output">{prometheusText}</pre>
			{:else}
				<LoadingState
					message="Load the current Prometheus payload to inspect or copy it."
				/>
			{/if}
		</Card>

		<Card title="Retention Cleanup" subtitle="Delete historical metric records">
			<p class="cleanup-warning">
				Deleted metric history cannot be recovered. Live system metrics are
				unaffected.
			</p>
			<div class="tool-form-row">
				<label class="filter-check">
					<input type="checkbox" bind:checked={cleanupAll} />
					<span>Delete all records</span>
				</label>
				<label class="form-field">
					<span class="form-label">Delete records before</span>
					<input
						class="text-input"
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
				<InlineError message={cleanupError} />
			{/if}
		</Card>
	</div>
</div>

{#if cleanupConfirmOpen}
	<Modal title="Confirm Metrics Cleanup" onClose={() => (cleanupConfirmOpen = false)}>
		<p>
			{cleanupAll
				? 'All historical metric records will be permanently deleted.'
				: `All metric records before ${cleanupBefore} will be permanently deleted.`}
		</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (cleanupConfirmOpen = false)}>Cancel</Button>
			<Button variant="danger" onclick={runCleanup} disabled={cleanupBusy}>
				{cleanupBusy ? 'Cleaning...' : 'Confirm Cleanup'}
			</Button>
		{/snippet}
	</Modal>
{/if}

<style>





	.filter-check {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.85rem;
		color: var(--gray-600);
		padding-bottom: 0.45rem;
	}


	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}

	th {
		font-family: var(--font-mono);
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-widest);
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



	.result-count {
		margin-top: 0.75rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
	}


	.prom-output {
		background: var(--gray-900);
		color: var(--gray-200);
		padding: 1rem;
		overflow: auto;
		max-height: 320px;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		line-height: 1.5;
	}

	.cleanup-warning {
		color: var(--gray-600);
		margin-bottom: 1rem;
		line-height: 1.6;
	}


	.inline-success {
		margin-top: 1rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--success);
		color: var(--success);
		font-family: var(--font-mono);
		font-size: 0.75rem;
	}





</style>
