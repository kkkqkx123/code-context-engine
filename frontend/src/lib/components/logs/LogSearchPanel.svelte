<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogEntry } from '$lib/api/logs';

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
</script>

<div class="p-4 space-y-4">
	<div class="grid grid-cols-2 gap-2">
		<select class="select" bind:value={level}>
			<option value="">All levels</option>
			<option value="TRACE">TRACE</option>
			<option value="DEBUG">DEBUG</option>
			<option value="INFO">INFO</option>
			<option value="WARN">WARN</option>
			<option value="ERROR">ERROR</option>
		</select>
		<input class="input" placeholder="module" bind:value={module} />
		<input class="input" placeholder="keyword" bind:value={query} />
		<input class="input" placeholder="start time (ISO)" bind:value={startTime} />
		<input class="input" placeholder="end time (ISO)" bind:value={endTime} />
		<input class="input" type="number" bind:value={pageSize} />
	</div>
	<div class="flex gap-2">
		<button type="button" class="bg-blue-500 text-white px-4 py-2 rounded" onclick={search} disabled={loading}>
			{loading ? 'Searching...' : 'Search'}
		</button>
		<button type="button" class="bg-gray-200 px-4 py-2 rounded" onclick={exportCsv}>Export CSV</button>
	</div>
	{#if error}
		<div class="text-red-600">{error}</div>
	{/if}
	<div class="text-sm text-gray-500">Total: {total}</div>
	<div class="overflow-auto max-h-[60vh] border">
		<table class="w-full text-xs font-mono">
			<thead>
				<tr class="bg-gray-100">
					<th class="p-1 text-left">Time</th>
					<th class="p-1 text-left">Level</th>
					<th class="p-1 text-left">Target</th>
					<th class="p-1 text-left">Message</th>
				</tr>
			</thead>
			<tbody>
				{#each entries as e, i (i)}
					<tr class="border-t">
						<td class="p-1">{e.timestamp}</td>
						<td class="p-1">{e.level}</td>
						<td class="p-1">{e.target}</td>
						<td class="p-1">{e.message}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<div class="flex gap-2 items-center">
		<button type="button" class="px-2 py-1 bg-gray-200 rounded" onclick={() => { if (page > 1) { page--; search(); } }}>Prev</button>
		<span>Page {page}</span>
		<button type="button" class="px-2 py-1 bg-gray-200 rounded" onclick={() => { page++; search(); }}>Next</button>
	</div>
</div>
