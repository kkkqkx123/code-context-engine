<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { wsLogsUrl, type LogEntry } from '$lib/api/logs';

	let entries = $state<LogEntry[]>([]);
	let paused = $state(false);
	let connected = $state(false);
	let levelFilter = $state<string[]>(['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR']);
	let keyword = $state('');
	let moduleFilter = $state('');
	let socket: WebSocket | null = null;

	const levelColors: Record<string, string> = {
		TRACE: 'text-gray-400',
		DEBUG: 'text-gray-500',
		INFO: 'text-green-600',
		WARN: 'text-yellow-600',
		ERROR: 'text-red-600',
	};

	function append(entry: LogEntry) {
		if (paused) return;
		if (!levelFilter.includes(entry.level)) return;
		if (moduleFilter && !entry.target.includes(moduleFilter)) return;
		if (keyword && !entry.message.includes(keyword) && !entry.target.includes(keyword)) return;
		entries = [...entries.slice(-499), entry];
	}

	onMount(() => {
		socket = new WebSocket(wsLogsUrl());
		socket.onopen = () => (connected = true);
		socket.onclose = () => (connected = false);
		socket.onmessage = (ev) => {
			try {
				const data = JSON.parse(ev.data) as LogEntry;
				append(data);
			} catch {
				// ignore
			}
		};
	});

	onDestroy(() => {
		socket?.close();
	});

	function toggleLevel(level: string) {
		if (levelFilter.includes(level)) {
			levelFilter = levelFilter.filter((l) => l !== level);
		} else {
			levelFilter = [...levelFilter, level];
		}
	}

	function clear() {
		entries = [];
	}
</script>

<div class="flex flex-col h-full">
	<div class="flex items-center gap-2 p-2 border-b">
		<span class={connected ? 'text-green-600' : 'text-red-600'}>
			{connected ? 'Connected' : 'Disconnected'}
		</span>
		{#each ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'] as level (level)}
			<button
				class="px-2 py-1 text-xs rounded {levelFilter.includes(level) ? 'bg-blue-500 text-white' : 'bg-gray-200'}"
				onclick={() => toggleLevel(level)}
			>
				{level}
			</button>
		{/each}
		<input
			class="border px-2 py-1 text-sm flex-1"
			placeholder="module filter"
			bind:value={moduleFilter}
		/>
		<input
			class="border px-2 py-1 text-sm flex-1"
			placeholder="keyword"
			bind:value={keyword}
		/>
		<button class="px-2 py-1 text-sm bg-gray-200 rounded" onclick={clear}>Clear</button>
		<button class="px-2 py-1 text-sm bg-gray-200 rounded" onclick={() => (paused = !paused)}>
			{paused ? 'Resume' : 'Pause'}
		</button>
	</div>
	<div class="flex-1 overflow-auto font-mono text-xs p-2 bg-black text-white">
		{#each entries as e, i (i)}
			<div class={levelColors[e.level] ?? 'text-white'}>
				[{e.timestamp}] {e.level} {e.target}: {e.message}
			</div>
		{/each}
	</div>
</div>
