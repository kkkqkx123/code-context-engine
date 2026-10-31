<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { wsLogsUrl, type LogEntry } from '$lib/api/logs';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';

	let entries = $state<LogEntry[]>([]);
	let paused = $state(false);
	let connected = $state(false);
	let levelFilter = $state<string[]>([
		'TRACE',
		'DEBUG',
		'INFO',
		'WARN',
		'ERROR',
	]);
	let keyword = $state('');
	let moduleFilter = $state('');
	let socket: WebSocket | null = null;

	type LevelVariant = 'default' | 'info' | 'success' | 'warning' | 'danger';

	function levelVariant(level: string): LevelVariant {
		switch (level) {
			case 'ERROR':
				return 'danger';
			case 'WARN':
				return 'warning';
			case 'INFO':
				return 'info';
			case 'DEBUG':
				return 'default';
			default:
				return 'inactive' as unknown as LevelVariant;
		}
	}

	function append(entry: LogEntry) {
		if (paused) return;
		if (!levelFilter.includes(entry.level)) return;
		if (moduleFilter && !entry.target.includes(moduleFilter)) return;
		if (
			keyword &&
			!entry.message.includes(keyword) &&
			!entry.target.includes(keyword)
		)
			return;
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

<div class="realtime">
	<div class="realtime-toolbar">
		<Badge
			label={connected ? 'Connected' : 'Disconnected'}
			variant={connected ? 'success' : 'danger'}
		/>
		<div class="level-filters" role="group" aria-label="Log level filter">
			{#each ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'] as level (level)}
				<Button
					variant="tab"
					size="xs"
					active={levelFilter.includes(level)}
					onclick={() => toggleLevel(level)}
				>
					{level}
				</Button>
			{/each}
		</div>
		<input
			class="text-input text-input--inline"
			placeholder="module filter"
			bind:value={moduleFilter}
			aria-label="Module filter"
		/>
		<input
			class="text-input text-input--inline"
			placeholder="keyword"
			bind:value={keyword}
			aria-label="Keyword filter"
		/>
		<Button variant="secondary" size="xs" onclick={clear}>Clear</Button>
		<Button variant="secondary" size="xs" onclick={() => (paused = !paused)}>
			{paused ? 'Resume' : 'Pause'}
		</Button>
	</div>
	<div class="realtime-log" role="log" aria-live="polite">
		{#if entries.length === 0}
			<EmptyState
				message={paused
					? 'Paused — resume to receive logs'
					: 'Waiting for log events…'}
			/>
		{:else}
			{#each entries as e, i (i)}
				<div class="log-line">
					<span class="log-time">[{e.timestamp}]</span>
					<Badge label={e.level} variant={levelVariant(e.level)} />
					<span class="log-target">{e.target}</span>
					<span class="log-message">{e.message}</span>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.realtime {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		height: 100%;
	}

	.realtime-toolbar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--black);
	}

	.level-filters {
		display: flex;
		border: 1px solid var(--gray-300);
		width: fit-content;
	}

	.realtime-log {
		background: var(--black);
		color: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
		line-height: 1.6;
		padding: 1rem;
		height: 480px;
		overflow-y: auto;
		border: 1px solid var(--black);
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.log-line {
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
		padding: 0.2rem 0;
		border-bottom: 1px solid var(--gray-800);
		min-width: 0;
	}

	.log-line:last-child {
		border-bottom: none;
	}

	.log-time {
		color: var(--gray-500);
		flex-shrink: 0;
	}

	.log-target {
		color: var(--gray-300);
		flex-shrink: 0;
		max-width: 280px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.log-message {
		color: var(--white);
		flex: 1;
		min-width: 0;
		word-break: break-word;
	}
</style>
