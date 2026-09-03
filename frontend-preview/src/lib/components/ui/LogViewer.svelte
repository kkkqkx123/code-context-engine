<script lang="ts">
	import { tick } from 'svelte';

	interface WatchEvent {
		timestamp: Date;
		eventType: string;
		filePath: string;
		action: string;
	}

	interface Props {
		events?: WatchEvent[];
		paused?: boolean;
	}

	let {
		events = [],
		paused = false
	}: Props = $props();

	let logContainer: HTMLDivElement | undefined = $state();

	// Auto-scroll to bottom when new events arrive (unless paused)
	$effect(() => {
		if (!paused && events.length > 0) {
			void scrollToBottom();
		}
	});

	async function scrollToBottom() {
		await tick();
		if (logContainer) {
			logContainer.scrollTop = logContainer.scrollHeight;
		}
	}

	function formatTime(date: Date): string {
		return date.toLocaleTimeString('en-US', {
			hour12: false,
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
		});
	}

	function getEventColor(eventType: string): string {
		switch (eventType.toLowerCase()) {
			case 'create':
				return 'var(--status-created)';
			case 'modify':
				return 'var(--status-modified)';
			case 'delete':
				return 'var(--status-deleted)';
			default:
				return 'var(--white)';
		}
	}
</script>

<div class="log-container" bind:this={logContainer}>
	{#if events.length === 0}
		<div class="empty-state">No events recorded</div>
	{:else}
		{#each events as event (event.timestamp.getTime())}
			<div class="log-entry">
				<span class="log-time">{formatTime(event.timestamp)}</span>
				<span
					class="log-event-type"
					style="color: {getEventColor(event.eventType)}"
				>
					[{event.eventType.toUpperCase()}]
				</span>
				<span class="log-file-path">{event.filePath}</span>
				<span class="log-action">→ {event.action}</span>
			</div>
		{/each}
	{/if}
</div>

<style>
	.log-container {
		background: var(--black);
		color: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		padding: 1rem;
		height: 400px;
		overflow-y: auto;
		border: 1px solid var(--black);
	}

	.log-container::-webkit-scrollbar {
		width: 8px;
	}

	.log-container::-webkit-scrollbar-track {
		background: var(--gray-800);
	}

	.log-container::-webkit-scrollbar-thumb {
		background: var(--gray-500);
	}

	.empty-state {
		color: var(--gray-500);
		text-align: center;
		padding: 2rem;
		font-style: italic;
	}

	.log-entry {
		display: flex;
		gap: 0.75rem;
		padding: 0.25rem 0;
		border-bottom: 1px solid var(--gray-800);
		line-height: 1.5;
	}

	.log-entry:last-child {
		border-bottom: none;
	}

	.log-time {
		color: var(--gray-500);
		min-width: 80px;
		flex-shrink: 0;
	}

	.log-event-type {
		font-weight: bold;
		min-width: 80px;
		flex-shrink: 0;
	}

	.log-file-path {
		color: var(--white);
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.log-action {
		color: var(--gray-500);
		flex-shrink: 0;
	}
</style>
