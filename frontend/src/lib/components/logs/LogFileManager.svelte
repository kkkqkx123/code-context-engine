<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogFileInfo } from '$lib/api/logs';

	let files = $state<LogFileInfo[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function refresh() {
		loading = true;
		error = null;
		try {
			files = await logsApi.listFiles();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function rotate() {
		try {
			await logsApi.rotate();
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	async function remove(name: string) {
		if (!confirm(`Delete ${name}?`)) return;
		try {
			await logsApi.deleteFile(name);
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(refresh);

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<div class="p-4 space-y-4">
	<div class="flex gap-2">
		<button type="button" class="bg-blue-500 text-white px-4 py-2 rounded" onclick={refresh} disabled={loading}>
			{loading ? 'Loading...' : 'Refresh'}
		</button>
		<button type="button" class="bg-yellow-500 text-white px-4 py-2 rounded" onclick={rotate}>Rotate Now</button>
	</div>
	{#if error}
		<div class="text-red-600">{error}</div>
	{/if}
	<div class="overflow-auto border">
		<table class="w-full text-sm">
			<thead class="bg-gray-100">
				<tr>
					<th class="p-2 text-left">Name</th>
					<th class="p-2 text-left">Size</th>
					<th class="p-2 text-left">Modified</th>
					<th class="p-2 text-left">Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each files as f (f.name)}
					<tr class="border-t">
						<td class="p-2">{f.name}</td>
						<td class="p-2">{formatSize(f.size)}</td>
						<td class="p-2">{f.modified}</td>
						<td class="p-2 flex gap-2">
							<a class="text-blue-600 underline" href={logsApi.downloadUrl(f.name)} target="_blank" rel="external">Download</a>
							<button type="button" class="text-red-600" onclick={() => remove(f.name)}>Delete</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
</div>
