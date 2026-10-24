<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogConfig } from '$lib/api/logs';

	let config = $state<LogConfig | null>(null);
	let level = $state('info');
	let format = $state('pretty');
	let filter = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);
	let message = $state<string | null>(null);

	async function load() {
		try {
			config = await logsApi.getConfig();
			level = config.level;
			format = config.format;
			filter = config.filter ?? '';
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	async function save() {
		saving = true;
		error = null;
		message = null;
		try {
			await logsApi.updateConfig({ level, format, filter: filter || undefined });
			message = 'Updated';
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	onMount(load);
</script>

<div class="p-4 space-y-4 max-w-xl">
	{#if config}
		<div class="space-y-2 text-sm text-gray-600">
			<div>Output: {config.output}</div>
			<div>File: {config.file ?? '-'}</div>
			<div>Max file size (MB): {config.max_file_size_mb ?? '-'}</div>
			<div>Max files: {config.max_files ?? '-'}</div>
			<div>Max total size (MB): {config.max_total_size_mb ?? '-'}</div>
			<div>WS capacity: {config.ws_capacity}</div>
			<div>Buffer size: {config.memory_buffer_size}</div>
		</div>
	{/if}
	{#if error}
		<div class="text-red-600">{error}</div>
	{/if}
	{#if message}
		<div class="text-green-600">{message}</div>
	{/if}
	<div>
		<label for="log-level" class="block text-sm mb-1">Level</label>
		<select id="log-level" class="border p-2 w-full" bind:value={level}>
			<option value="trace">trace</option>
			<option value="debug">debug</option>
			<option value="info">info</option>
			<option value="warn">warn</option>
			<option value="error">error</option>
		</select>
	</div>
	<div>
		<label for="log-format" class="block text-sm mb-1">Format</label>
		<select id="log-format" class="border p-2 w-full" bind:value={format}>
			<option value="pretty">pretty</option>
			<option value="compact">compact</option>
			<option value="json">json</option>
		</select>
	</div>
	<div>
		<label for="log-filter" class="block text-sm mb-1">Filter directives</label>
		<input id="log-filter" class="border p-2 w-full" bind:value={filter} placeholder="info,cce_server=debug" />
	</div>
	<button class="bg-blue-500 text-white px-4 py-2 rounded" onclick={save} disabled={saving}>
		{saving ? 'Saving...' : 'Save'}
	</button>
</div>
