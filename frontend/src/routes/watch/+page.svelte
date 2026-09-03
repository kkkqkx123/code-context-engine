<script lang="ts">
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { watchState, watchActions } from '$lib/stores/watch';

	// Lazy load LogViewer component
	let LogViewer: any = $state(null);
	let logViewerLoaded = $state(false);

	let watchPath = $state('');
	let extensions = $state('ts,js,tsx,jsx,rs,py,go');
	let debounceMs = $state(1000);
	let isPaused = $state(false);

	onMount(() => {
		watchActions.loadStatus();
		// Preload LogViewer component
		loadLogViewerComponent();
	});

	async function handleStartWatch() {
		if (!watchPath) return;
		const extArray = extensions.split(',').map(e => e.trim()).filter(e => e);
		await watchActions.startWatch(watchPath, extArray, debounceMs);
	}

	async function handleStopWatch() {
		await watchActions.stopWatch();
	}

	function togglePause() {
		isPaused = !isPaused;
	}

	function clearEvents() {
		watchActions.clearEvents();
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
	<title>File Watcher - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>File Watcher</h1>
		<p class="page-description">Monitor file system changes for automatic incremental indexing</p>

		{#if $watchState.error}
			<div class="error-banner">
				<span>{$watchState.error}</span>
				<button class="dismiss-btn" onclick={() => watchState.update(s => ({...s, error: null}))}>×</button>
			</div>
		{/if}

		<!-- Watch Control Panel -->
		<Card title="Watch Control" subtitle="Configure and start file monitoring">
			<div class="control-grid">
				<div class="control-group">
					<label class="field-label" for="watch-path">Directory Path</label>
					<Input
						id="watch-path"
						type="text"
						bind:value={watchPath}
						placeholder="/path/to/project"
						disabled={$watchState.isWatching}
					/>
				</div>

				<div class="control-group">
					<label class="field-label" for="file-extensions">File Extensions (comma-separated)</label>
					<Input
						id="file-extensions"
						type="text"
						bind:value={extensions}
						placeholder="ts,js,rs,py"
						disabled={$watchState.isWatching}
					/>
				</div>

				<div class="control-group">
					<label class="field-label" for="debounce-interval">Debounce Interval (ms)</label>
					<input
						id="debounce-interval"
						type="number"
						bind:value={debounceMs}
						min="100"
						max="5000"
						step="100"
						class="debounce-input"
						disabled={$watchState.isWatching}
					/>
				</div>

				<div class="control-group control-actions">
					{#if $watchState.isWatching}
						<Button variant="secondary" onclick={handleStopWatch} disabled={$watchState.isLoading}>
							{#if $watchState.isLoading}Stopping...{:else}Stop Watching{/if}
						</Button>
					{:else}
						<Button onclick={handleStartWatch} disabled={!watchPath || $watchState.isLoading}>
							{#if $watchState.isLoading}Starting...{:else}Start Watching{/if}
						</Button>
					{/if}
				</div>
			</div>

			{#if $watchState.status}
				<div class="status-bar">
					<div class="status-indicator">
						<span class="live-dot" class:active={$watchState.isWatching}></span>
						<Badge
							label={$watchState.isWatching ? 'Active' : 'Inactive'}
							variant={$watchState.isWatching ? 'active' : 'inactive'}
						/>
					</div>
					<div class="status-info">
						<span class="info-item">Events: {$watchState.status.events_processed}</span>
						{#if $watchState.status.watched_dirs && $watchState.status.watched_dirs.length > 0}
							<span class="info-item">Watching: {$watchState.status.watched_dirs.length} dir(s)</span>
						{/if}
						{#if $watchState.status.started_at}
							<span class="info-item">Started: {new Date($watchState.status.started_at).toLocaleString()}</span>
						{/if}
					</div>
				</div>
			{/if}
		</Card>

		<!-- Live Event Feed -->
		<Card title="Event Feed" subtitle="Real-time file system events">
			<div class="feed-controls">
				<Button
					variant="secondary"
					onclick={togglePause}
				>
					{isPaused ? 'Resume' : 'Pause'}
				</Button>
				<Button
					variant="secondary"
					onclick={clearEvents}
				>
					Clear
				</Button>
				<span class="event-count">{$watchState.events.length} events</span>
			</div>

			{#if LogViewer}
				<LogViewer events={$watchState.events} paused={isPaused} />
			{:else}
				<div class="loading-spinner">Loading log viewer...</div>
			{/if}
		</Card>
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 3rem;
	}

	.error-banner {
		background: var(--danger);
		color: var(--white);
		padding: 1rem;
		margin-bottom: 2rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		border: 1px solid var(--black);
	}

	.dismiss-btn {
		background: none;
		border: none;
		color: var(--white);
		font-size: 1.5rem;
		cursor: pointer;
		line-height: 1;
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
		background: var(--accent);
		animation: pulse 2s infinite;
	}

	@keyframes pulse {
		0%, 100% {
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

	.loading-spinner {
		padding: 2rem;
		text-align: center;
		color: var(--gray-600);
		font-family: 'Space Mono', monospace;
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
