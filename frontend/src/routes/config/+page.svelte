<script lang="ts">
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { configApi, type ConfigInfoResponse, type ConfigValidateResponse } from '$lib/api/config';
	import { currentProjectId } from '$lib/stores/project';

	let activeTab = $state<'info' | 'validate' | 'reload'>('info');
	let configInfo = $state<ConfigInfoResponse | null>(null);
	let validateResult = $state<ConfigValidateResponse | null>(null);
	let reloadResult = $state<{ success: boolean; message: string } | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	onMount(async () => {
		await loadInfo();
		await loadValidate();
	});

	async function loadInfo() {
		loading = true;
		error = null;
		try {
			configInfo = await configApi.getInfo();
		} catch (e: any) {
			error = e.message || 'Failed to load config';
		} finally {
			loading = false;
		}
	}

	async function loadValidate() {
		try {
			validateResult = await configApi.validate();
		} catch (_) {
			// silent
		}
	}

	async function handleReload() {
		loading = true;
		error = null;
		reloadResult = null;
		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();
			reloadResult = await configApi.reload(pid!);
		} catch (e: any) {
			error = e.message || 'Failed to reload config';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>Configuration - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Configuration</h1>
		<p class="page-description">Manage application settings and environment variables</p>

		{#if error}
			<div class="error-banner">
				<span>{error}</span>
				<button class="dismiss-btn" onclick={() => error = null}>×</button>
			</div>
		{/if}

		<!-- Tab Navigation -->
		<div class="tab-nav">
			<button
				class="tab-btn"
				class:active={activeTab === 'info'}
				onclick={() => activeTab = 'info'}
			>
				Config Info
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'validate'}
				onclick={() => activeTab = 'validate'}
			>
				Validate
			</button>
			<button
				class="tab-btn"
				class:active={activeTab === 'reload'}
				onclick={() => activeTab = 'reload'}
			>
				Reload
			</button>
		</div>

		<!-- Config Info Tab -->
		{#if activeTab === 'info'}
			<Card title="Configuration Info" subtitle="Current active configuration">
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
						<pre class="config-json">{JSON.stringify(configInfo.database, null, 2)}</pre>
					</div>

					<div class="config-section">
						<h3 class="section-title">Embedder</h3>
						<pre class="config-json">{JSON.stringify(configInfo.embedder, null, 2)}</pre>
					</div>
				{:else}
					<p class="placeholder-text">No configuration data available</p>
				{/if}
			</Card>
		{/if}

		<!-- Validate Tab -->
		{#if activeTab === 'validate'}
			<Card title="Configuration Validation" subtitle="Check configuration for issues">
				{#if validateResult}
					<div class="validate-status">
						<span class="validate-label">Status</span>
						<Badge
							label={validateResult.valid ? 'Valid' : 'Invalid'}
							variant={validateResult.valid ? 'active' : 'inactive'}
						/>
					</div>

					{#if validateResult.errors.length > 0}
						<div class="issue-section">
							<h3 class="section-title">Errors ({validateResult.errors.length})</h3>
							<ul class="issue-list">
								{#each validateResult.errors as err}
									<li class="issue-item error">{err}</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.warnings.length > 0}
						<div class="issue-section">
							<h3 class="section-title">Warnings ({validateResult.warnings.length})</h3>
							<ul class="issue-list">
								{#each validateResult.warnings as warn}
									<li class="issue-item warning">{warn}</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.dependency_warnings.length > 0}
						<div class="issue-section">
							<h3 class="section-title">Dependency Warnings ({validateResult.dependency_warnings.length})</h3>
							<ul class="issue-list">
								{#each validateResult.dependency_warnings as dw}
									<li class="issue-item warning">
										<strong>{dw.module}:</strong> {dw.message}
									</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.valid && validateResult.errors.length === 0 && validateResult.warnings.length === 0 && validateResult.dependency_warnings.length === 0}
						<p class="placeholder-text">No issues found — configuration is clean.</p>
					{/if}
				{:else}
					<p class="placeholder-text">Loading validation results...</p>
				{/if}
			</Card>
		{/if}

		<!-- Reload Tab -->
		{#if activeTab === 'reload'}
			<Card title="Reload Configuration" subtitle="Trigger a configuration reload">
				<p class="reload-description">
					This will reload the configuration for the current project from disk.
					Any pending changes to configuration files will be applied.
				</p>

				<div class="reload-actions">
					<Button onclick={handleReload} disabled={loading}>
						{#if loading}Reloading...{:else}Reload Configuration{/if}
					</Button>
				</div>

				{#if reloadResult}
					<div class="reload-result">
						<Badge
							label={reloadResult.success ? 'Success' : 'Failed'}
							variant={reloadResult.success ? 'active' : 'inactive'}
						/>
						<span class="reload-message">{reloadResult.message}</span>
					</div>
				{/if}
			</Card>
		{/if}
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 2rem;
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

	/* Tab Navigation */
	.tab-nav {
		display: flex;
		gap: 0;
		border-bottom: 2px solid var(--black);
		margin-bottom: 2rem;
	}

	.tab-btn {
		padding: 1rem 2rem;
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -2px;
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		transition: all 0.3s ease;
		color: var(--gray-600);
	}

	.tab-btn:hover {
		color: var(--black);
	}

	.tab-btn.active {
		color: var(--black);
		border-bottom-color: var(--accent);
		font-weight: bold;
	}

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
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 2rem;
	}

	.validate-status {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}

	.validate-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.issue-section {
		margin-top: 1.5rem;
	}

	.issue-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.issue-item {
		padding: 0.75rem 1rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		border: 1px solid var(--gray-200);
		line-height: 1.5;
	}

	.issue-item.error {
		border-color: var(--danger);
		color: var(--danger);
		background: var(--danger-bg, #fff5f5);
	}

	.issue-item.warning {
		border-color: var(--warning, #e6a700);
		color: var(--warning-text, #8a6d00);
		background: var(--warning-bg, #fffbe6);
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

	.reload-result {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem;
		border: 1px solid var(--gray-200);
	}

	.reload-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}
</style>