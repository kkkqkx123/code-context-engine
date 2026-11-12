<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogEntry } from '$lib/api/logs';
	import Button from '$lib/components/ui/Button.svelte';
	import Pagination from '$lib/components/ui/Pagination.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Toolbar from '$lib/components/ui/Toolbar.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';

	let entries = $state<LogEntry[]>([]);
	let total = $state(0);
	let level = $state('');
	let module = $state('');
	let query = $state('');
	let startTime = $state('');
	let endTime = $state('');
	let page = $state(1);
	let pageSize = $state(100);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function search() {
		loading = true;
		error = null;
		try {
			const res = await logsApi.getLogs({
				level: level || undefined,
				module: module || undefined,
				query: query || undefined,
				start_time: startTime || undefined,
				end_time: endTime || undefined,
				page,
				page_size: pageSize,
			});
			entries = res.entries;
			total = res.total;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	onMount(search);

	function exportCsv() {
		const rows = ['timestamp,level,target,message'];
		for (const e of entries) {
			rows.push(
				`"${e.timestamp}","${e.level}","${e.target}","${e.message.replace(/"/g, '""')}"`,
			);
		}
		const blob = new Blob([rows.join('\n')], { type: 'text/csv' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'logs.csv';
		a.click();
		URL.revokeObjectURL(url);
	}

	type LevelVariant = 'default' | 'info' | 'success' | 'warning' | 'danger';

	function levelVariant(raw: string): LevelVariant {
		switch (raw) {
			case 'ERROR':
				return 'danger';
			case 'WARN':
				return 'warning';
			case 'INFO':
				return 'info';
			default:
				return 'default';
		}
	}
</script>

<Card
	title="Search Filters"
	subtitle="Query historical logs by level, module and time"
>
	<div class="form-grid">
		<label class="form-field">
			<span class="form-label">Level</span>
			<select class="text-input" bind:value={level} aria-label="Log level">
				<option value="">All levels</option>
				<option value="TRACE">TRACE</option>
				<option value="DEBUG">DEBUG</option>
				<option value="INFO">INFO</option>
				<option value="WARN">WARN</option>
				<option value="ERROR">ERROR</option>
			</select>
		</label>
		<label class="form-field">
			<span class="form-label">Module</span>
			<input class="text-input" placeholder="module" bind:value={module} />
		</label>
		<label class="form-field">
			<span class="form-label">Keyword</span>
			<input class="text-input" placeholder="keyword" bind:value={query} />
		</label>
		<label class="form-field">
			<span class="form-label">Start time (ISO)</span>
			<input
				class="text-input"
				placeholder="start time (ISO)"
				bind:value={startTime}
			/>
		</label>
		<label class="form-field">
			<span class="form-label">End time (ISO)</span>
			<input
				class="text-input"
				placeholder="end time (ISO)"
				bind:value={endTime}
			/>
		</label>
		<label class="form-field">
			<span class="form-label">Page size</span>
			<input class="text-input" type="number" bind:value={pageSize} />
		</label>
	</div>
	{#if error}
		<InlineError message={error} />
	{/if}
	<Toolbar>
		<span class="total">Total: {total}</span>
		{#snippet actions()}
			<Button variant="secondary" onclick={exportCsv}>Export CSV</Button>
			<Button onclick={() => search()} disabled={loading}>
				{loading ? 'Searching...' : 'Search'}
			</Button>
		{/snippet}
	</Toolbar>
</Card>

<Card title="Results" subtitle={`${entries.length} entries on page ${page}`}>
	{#if entries.length === 0}
		<EmptyState message="No log entries match the current filters." />
	{:else}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Time</th>
						<th>Level</th>
						<th>Target</th>
						<th>Message</th>
					</tr>
				</thead>
				<tbody>
					{#each entries as e, i (i)}
						<tr>
							<td class="font-mono">{e.timestamp}</td>
							<td><Badge label={e.level} variant={levelVariant(e.level)} /></td>
							<td class="font-mono">{e.target}</td>
							<td>{e.message}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
	<Pagination
		page={page}
		size="xs"
		prevLabel="Prev"
		nextLabel="Next"
		onPrev={() => {
			if (page > 1) {
				page--;
				search();
			}
		}}
		onNext={() => {
			page++;
			search();
		}}
		prevDisabled={page <= 1}
		nextDisabled={entries.length < pageSize}
	/>
</Card>

<style>



	.total {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-600);
	}


</style>
