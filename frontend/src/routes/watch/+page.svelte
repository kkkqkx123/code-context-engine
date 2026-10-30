<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import type { Component } from 'svelte';
	import { onDestroy, onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import { watchStore } from '$lib/stores/watch.svelte';

	// Lazy load LogViewer component
	let LogViewer: Component | null = $state(null);
	let logViewerLoaded = $state(false);

	let watchPath = $state('');
	let extensions = $state('ts,js,tsx,jsx,rs,py,go');
	let debounceMs = $state(1000);
	let isPaused = $state(false);

	onMount(() => {
		watchStore.loadStatus();
		watchStore.startVersionPoll();
		// Preload LogViewer component
		loadLogViewerComponent();
	});

	onDestroy(() => {
		watchStore.stopVersionPoll();
	});

	async function handleStartWatch() {
		if (!watchPath) return;
		const extArray = extensions
			.split(',')
			.map((e) => e.trim())
			.filter((e) => e);
		await watchStore.startWatch(watchPath, extArray, debounceMs);
	}

	async function handleStopWatch() {
		await watchStore.stopWatch();
	}

	function togglePause() {
		isPaused = !isPaused;
	}

	function clearEvents() {
		watchStore.clearEvents();
	}

	async function loadLogViewerComponent() {
		if (!logViewerLoaded) {
			const module = await import('$lib/components/ui/LogViewer.svelte');
			LogViewer = module.default;
			logViewerLoaded = true;
		}
	}
</script>

<svelte:head>
	<title>Local File Watcher - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Local File Watcher (Same Machine)"
			subtitle="Server-side local watching only. Remote projects use gateway push from the machine where files live; this page cannot start gateway supply."
		/>

		{#if watchStore.error}
			<ErrorBanner>
				<span>{watchStore.error}</span>
				<button type="button"
					class="dismiss-btn"
					onclick={() => (watchStore.error = null)}
					><Icon name="close" /></button
				>
			</ErrorBanner>
		{/if}

		{#if watchStore.error && watchStore.error.includes('gateway-supplied')}
			<div class="gateway-hint">
				<span
					>This project is gateway-supplied; local watching is disabled. Push changes
					with `cce-cli gateway watch` from the machine where the files live.</span
				>
			</div>
		{/if}

		<!-- Watch Control Panel -->
		<Card
			title="Watch Control"
			subtitle="Server-side local watching (requires same machine)"
		>
			<div class="control-grid">
				<div class="control-group">
					<label class="field-label" for="watch-path">Directory Path</label>
					<Input
						id="watch-path"
						type="text"
						bind:value={watchPath}
						placeholder="/path/to/project"
						disabled={watchStore.isWatching}
					/>
				</div>

				<div class="control-group">
					<label class="field-label" for="file-extensions"
						>File Extensions (comma-separated)</label
					>
					<Input
						id="file-extensions"
						type="text"
						bind:value={extensions}
						placeholder="ts,js,rs,py"
						disabled={watchStore.isWatching}
					/>
				</div>

				<div class="control-group">
					<label class="field-label" for="debounce-interval"
						>Debounce Interval (ms)</label
					>
					<input
						id="debounce-interval"
						type="number"
						bind:value={debounceMs}
						min="100"
						max="5000"
						step="100"
						class="debounce-input"
						disabled={watchStore.isWatching}
					/>
				</div>

				<div class="control-group control-actions">
					{#if watchStore.isWatching}
						<Button
							variant="secondary"
							onclick={handleStopWatch}
							disabled={watchStore.isLoading}
						>
							{#if watchStore.isLoading}Stopping...{:else}Stop Watching{/if}
						</Button>
					{:else}
						<Button
							onclick={handleStartWatch}
							disabled={!watchPath || watchStore.isLoading}
						>
							{#if watchStore.isLoading}Starting...{:else}Start Watching{/if}
						</Button>
					{/if}
				</div>
			</div>

			{#if watchStore.status}
				<div class="status-bar">
					<div class="status-indicator">
						<span class="live-dot" class:active={watchStore.isWatching}></span>
						<Badge
							label={watchStore.isWatching ? 'Active' : 'Inactive'}
							variant={watchStore.isWatching ? 'success' : 'inactive'}
						/>
					</div>
					<div class="status-info">
						<span class="info-item"
							>Events: {watchStore.status.events_processed}</span
						>
						{#if watchStore.status.watched_dirs && watchStore.status.watched_dirs.length > 0}
							<span class="info-item"
								>Watching: {watchStore.status.watched_dirs.length} dir(s)</span
							>
						{/if}
						{#if watchStore.status.started_at}
							<span class="info-item"
								>Started: {new Date(
									watchStore.status.started_at,
								).toLocaleString()}</span
							>
						{/if}
						{#if watchStore.status.relation_epoch !== undefined && watchStore.status.relation_epoch !== null}
							<span class="info-item"
								>Relation epoch: {watchStore.status.relation_epoch}</span
							>
						{/if}
						{#if (watchStore.status as unknown as { data_epoch?: number | null }).data_epoch !== undefined && (watchStore.status as unknown as { data_epoch?: number | null }).data_epoch !== null}
							<span class="info-item"
								>Data epoch: {(watchStore.status as unknown as { data_epoch?: number | null }).data_epoch}</span
							>
						{/if}
					</div>
				</div>
			{/if}
		</Card>

		<!-- Live Event Feed -->
		<Card
			title="Event Feed"
			subtitle="No live stream by design — progress comes from status polling (events_processed + epochs). For gateway projects freshness comes from gateway push, not local watching."
		>
			<div class="feed-controls">
				<Button variant="secondary" onclick={togglePause}>
					{isPaused ? 'Resume' : 'Pause'}
				</Button>
				<Button variant="secondary" onclick={clearEvents}>Clear</Button>
				<span class="event-count">{watchStore.events.length} events</span>
			</div>

			{#if LogViewer}
				<LogViewer events={watchStore.events} paused={isPaused} />
			{:else}
				<LoadingState message="Loading log viewer..." />
			{/if}
		</Card>
	</div>
</div>

<style>

	.dismiss-btn {
		background: none;
		border: none;
		color: var(--white);
		font-size: 1.5rem;
		cursor: pointer;
		line-height: 1;
	}

	.gateway-hint {
		background: var(--gray-100);
		color: var(--black);
		padding: 1rem;
		margin-bottom: 2rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}

	.control-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 1.5rem;
		margin-bottom: 1.5rem;
	}

	.control-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.debounce-input {
		width: 100%;
		padding: 0.75rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.9rem;
		background: var(--white);
	}

	.debounce-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.debounce-input:disabled {
		background: var(--gray-100);
		cursor: not-allowed;
	}

	.control-actions {
		justify-content: flex-end;
	}

	.status-bar {
		border-top: 1px solid var(--gray-200);
		padding-top: 1.5rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 1rem;
	}

	.status-indicator {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.live-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--gray-400);
		transition: background 0.3s ease;
	}

	.live-dot.active {
		background: var(--success);
		animation: pulse 2s infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}

	.status-info {
		display: flex;
		gap: 1.5rem;
		flex-wrap: wrap;
	}

	.info-item {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.feed-controls {
		display: flex;
		gap: 1rem;
		align-items: center;
		margin-bottom: 1rem;
	}

	.event-count {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
		margin-left: auto;
	}

	@media (max-width: 1024px) {
		.control-grid {
			grid-template-columns: 1fr;
		}

		.status-bar {
			flex-direction: column;
			align-items: flex-start;
		}
	}
</style>
