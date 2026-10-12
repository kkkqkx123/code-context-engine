<script lang="ts">
	/**
	 * Global configuration snapshot card for the config page. Read-only
	 * server state: the page owns loading and refresh.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import type { ConfigInfoResponse } from '$lib/api/config';

	interface Props {
		configInfo: ConfigInfoResponse | null;
		loading?: boolean;
		onRefresh: () => void;
	}

	let { configInfo, loading = false, onRefresh }: Props = $props();
</script>

<Card title="Global Configuration" subtitle="Active server snapshot (read-only)">
	{#if loading && !configInfo}
		<p class="placeholder-text">Loading configuration...</p>
	{:else if configInfo}
		<div class="config-grid">
			<div class="config-item">
				<span class="config-label">Initialized</span>
				<Badge
					label={configInfo.initialized ? 'Yes' : 'No'}
					variant={configInfo.initialized ? 'active' : 'inactive'}
				/>
			</div>
			<div class="config-item">
				<span class="config-label">Projects</span>
				<span class="config-value">{configInfo.project_count}</span>
			</div>
		</div>

		<div class="config-section">
			<h3 class="section-title">Database</h3>
			<pre class="config-json">{JSON.stringify(
					configInfo.database,
					null,
					2,
				)}</pre>
		</div>

		<div class="config-section">
			<h3 class="section-title">Embedder</h3>
			<pre class="config-json">{JSON.stringify(
					configInfo.embedder,
					null,
					2,
				)}</pre>
		</div>

		<div class="config-section">
			<h3 class="section-title">Full Snapshot</h3>
			<p class="reload-description">
				Complete merged server configuration including scanner, grouper,
				orchestrator, relation, LLM and plugin sections. Edit the
				configuration file on the server host and restart to change it.
			</p>
			<pre class="config-json">{JSON.stringify(
					(configInfo as Record<string, unknown>).config ?? {},
					null,
					2,
				)}</pre>
		</div>

		<div class="reload-actions">
			<Button onclick={onRefresh} disabled={loading}>
				{#if loading}Refreshing...{:else}Refresh Snapshot{/if}
			</Button>
		</div>
	{:else}
		<p class="placeholder-text">No configuration data available</p>
	{/if}
</Card>

<style>
	.config-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 1rem;
		margin-bottom: 1.5rem;
	}

	.config-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem;
		border: 1px solid var(--gray-200);
	}

	.config-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.config-value {
		font-family: 'Space Mono', monospace;
		font-size: 1rem;
		font-weight: 700;
	}

	.config-section {
		margin-top: 1.5rem;
	}

	.section-title {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		font-weight: 700;
		margin-bottom: 0.75rem;
		letter-spacing: -0.03em;
	}

	.config-json {
		background: var(--gray-50);
		border: 1px solid var(--gray-200);
		padding: 1rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		overflow-x: auto;
		white-space: pre-wrap;
		line-height: 1.5;
		max-height: 480px;
		overflow-y: auto;
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 2rem;
	}

	.reload-description {
		color: var(--gray-600);
		margin-bottom: 1.5rem;
		line-height: 1.6;
	}

	.reload-actions {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 1.5rem;
	}
</style>
