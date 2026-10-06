<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import {
		configApi,
		type ConfigInfoResponse,
		type ConfigValidateResponse,
	} from '$lib/api/config';
	import { projectApi, type ProjectConfigUpdateResponse } from '$lib/api';
	import { currentProjectId, onProjectChange } from '$lib/stores/project';
	import { get } from 'svelte/store';
	import { errorMessage } from '$lib/utils/errors';

	// Scope-based navigation: global state is read-only, project state is editable.
	// Edit, validation feedback and reload live in the same scope so no tab switch
	// is needed to confirm an applied change.
	let activeScope = $state<'global' | 'project'>('global');
	let configInfo = $state<ConfigInfoResponse | null>(null);
	let validateResult = $state<ConfigValidateResponse | null>(null);
	let loading = $state(false);
	let validating = $state(false);
	let error = $state<string | null>(null);

	// ─── Project basic metadata (SQLite-backed, via PUT /api/project/{id}) ──
	let basicLoaded = $state(false);
	let basicLoading = $state(false);
	let basicError = $state<string | null>(null);
	let basicResult = $state<string | null>(null);
	let basicSaving = $state(false);
	let rootPath = $state('');
	let basicName = $state('');
	let basicExtensions = $state('');
	let basicExcludeDirs = $state('');
	let basicIgnorePatterns = $state('');
	let basicRespectGitignore = $state(true);

	// ─── Project runtime config (merged AppConfig, via /api/project/{id}/config) ──
	let runtimeText = $state('');
	let runtimeLoaded = $state(false);
	let runtimeLoading = $state(false);
	let runtimeError = $state<string | null>(null);
	let runtimeResult = $state<ProjectConfigUpdateResponse | null>(null);
	let runtimeSaving = $state(false);
	let runtimeVersion = $state<number | null>(null);
	let reloadMessage = $state<string | null>(null);
	let reloading = $state(false);

	function parseList(value: string): string[] {
		return value
			.split(',')
			.map((item) => item.trim())
			.filter(Boolean);
	}

	async function loadInfo() {
		loading = true;
		error = null;
		try {
			configInfo = await configApi.getInfo();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	}

	async function loadValidate() {
		validating = true;
		try {
			validateResult = await configApi.validate();
		} catch {
			// Validation is advisory; the page stays usable without it.
		} finally {
			validating = false;
		}
	}

	async function loadProjectBasics() {
		basicLoading = true;
		basicError = null;
		try {
			const detail = await projectApi.getProject(String(get(currentProjectId)));
			const project = detail.project;
			rootPath = project.root_path;
			basicName = project.name;
			basicExtensions = (project.extensions ?? []).join(', ');
			basicExcludeDirs = (project.exclude_dirs ?? []).join(', ');
			basicIgnorePatterns = (project.ignore_patterns ?? []).join(', ');
			basicRespectGitignore = project.respect_gitignore ?? true;
			basicLoaded = true;
			basicResult = null;
		} catch (e) {
			basicError = errorMessage(e);
		} finally {
			basicLoading = false;
		}
	}

	async function loadProjectRuntime() {
		runtimeLoading = true;
		runtimeError = null;
		try {
			const response = await projectApi.getProjectConfig(
				String(get(currentProjectId)),
			);
			runtimeText = JSON.stringify(response.config, null, 2);
			runtimeVersion = response.config_version;
			runtimeLoaded = true;
			runtimeResult = null;
		} catch (e) {
			runtimeError = errorMessage(e);
		} finally {
			runtimeLoading = false;
		}
	}

	function ensureProjectLoaded() {
		if (!basicLoaded && !basicLoading) void loadProjectBasics();
		if (!runtimeLoaded && !runtimeLoading) void loadProjectRuntime();
	}

	async function saveProjectBasics() {
		basicError = null;
		basicResult = null;
		if (!basicName.trim()) {
			basicError = 'Project name is required';
			return;
		}
		basicSaving = true;
		try {
			await projectApi.updateProject(String($currentProjectId), {
				name: basicName.trim(),
				extensions: parseList(basicExtensions),
				exclude_dirs: parseList(basicExcludeDirs),
				respect_gitignore: basicRespectGitignore,
				ignore_patterns: parseList(basicIgnorePatterns),
			});
			basicResult = 'Project metadata saved';
		} catch (e) {
			basicError = errorMessage(e);
		} finally {
			basicSaving = false;
		}
	}

	async function saveProjectRuntime() {
		runtimeError = null;
		runtimeResult = null;

		let parsed: Record<string, unknown>;
		try {
			parsed = JSON.parse(runtimeText);
		} catch {
			runtimeError = 'Invalid JSON: fix syntax errors before saving';
			return;
		}

		runtimeSaving = true;
		try {
			runtimeResult = await projectApi.updateProjectConfig(
				String($currentProjectId),
				parsed,
			);
			// Refresh the version stamp and the global validation summary next
			// to the editor so the effect of the save is visible in place.
			void loadProjectRuntimeVersion();
			void loadValidate();
		} catch (e) {
			runtimeError = errorMessage(e);
		} finally {
			runtimeSaving = false;
		}
	}

	async function loadProjectRuntimeVersion() {
		try {
			const response = await projectApi.getProjectConfig(
				String(get(currentProjectId)),
			);
			runtimeVersion = response.config_version;
		} catch {
			// Version refresh is best-effort; the save result already landed.
		}
	}

	async function handleReloadFromFile() {
		reloading = true;
		runtimeError = null;
		reloadMessage = null;
		try {
			const response = await projectApi.reloadProject(
				String(get(currentProjectId)),
			);
			runtimeVersion = response.config_version;
			reloadMessage = response.message;
			// Reloading from disk can change both panes, refresh them together.
			void loadProjectBasics();
			void loadProjectRuntime();
		} catch (e) {
			runtimeError = errorMessage(e);
		} finally {
			reloading = false;
		}
	}

	onMount(async () => {
		await loadInfo();
		await loadValidate();
	});

	// Both project panes track one project; reload them after a switch.
	$effect(() =>
		onProjectChange(() => {
			if (basicLoaded) void loadProjectBasics();
			if (runtimeLoaded) void loadProjectRuntime();
		}),
	);
</script>

<svelte:head>
	<title>Configuration - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Configuration"
			subtitle="Global state is read-only; project state is editable per project"
		/>

		{#if error}
			<div class="error-banner">
				<span>{error}</span>
				<button class="dismiss-btn" onclick={() => (error = null)}>×</button>
			</div>
		{/if}

		<!-- Scope Navigation -->
		<div class="tab-nav">
			<button
				class="tab-btn"
				class:active={activeScope === 'global'}
				onclick={() => (activeScope = 'global')}
			>
				Global
			</button>
			<button
				class="tab-btn"
				class:active={activeScope === 'project'}
				onclick={() => {
					activeScope = 'project';
					ensureProjectLoaded();
				}}
			>
				Project #{ $currentProjectId }
			</button>
		</div>

		{#if activeScope === 'global'}
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
						<Button onclick={loadInfo} disabled={loading}>
							{#if loading}Refreshing...{:else}Refresh Snapshot{/if}
						</Button>
					</div>
				{:else}
					<p class="placeholder-text">No configuration data available</p>
				{/if}
			</Card>

			<Card title="Global Validation" subtitle="Cross-module checks for the active config">
				{#if validateResult}
					<div class="validate-status">
						<span class="validate-label">Status</span>
						<Badge
							label={validateResult.valid ? 'Valid' : 'Invalid'}
							variant={validateResult.valid ? 'active' : 'inactive'}
						/>
						<span class="validate-spacer"></span>
						<Button onclick={loadValidate} disabled={validating}>
							{#if validating}Checking...{:else}Re-validate{/if}
						</Button>
					</div>

					{#if validateResult.errors.length > 0}
						<div class="issue-section">
							<h3 class="section-title">
								Errors ({validateResult.errors.length})
							</h3>
							<ul class="issue-list">
								{#each validateResult.errors as err (err)}
									<li class="issue-item error">{err}</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.warnings.length > 0}
						<div class="issue-section">
							<h3 class="section-title">
								Warnings ({validateResult.warnings.length})
							</h3>
							<ul class="issue-list">
								{#each validateResult.warnings as warn (warn)}
									<li class="issue-item warning">{warn}</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.dependency_warnings.length > 0}
						<div class="issue-section">
							<h3 class="section-title">
								Dependency Warnings ({validateResult.dependency_warnings
									.length})
							</h3>
							<ul class="issue-list">
								{#each validateResult.dependency_warnings as dw (dw.field)}
									<li class="issue-item warning">
										<strong>{dw.field}:</strong>
										{dw.suggestion}
									</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if validateResult.valid && validateResult.errors.length === 0 && validateResult.warnings.length === 0 && validateResult.dependency_warnings.length === 0}
						<p class="placeholder-text">
							No issues found — configuration is clean.
						</p>
					{/if}
				{:else}
					<p class="placeholder-text">Loading validation results...</p>
				{/if}
			</Card>
		{/if}

		{#if activeScope === 'project'}
			<Card
				title="Project Basics"
				subtitle="Identity and file filters stored in the project registry"
			>
				{#if basicLoading && !basicLoaded}
					<p class="placeholder-text">Loading project...</p>
				{:else}
					<p class="reload-description">
						Root path is managed at creation time and cannot be changed here:
						<code>{rootPath || '—'}</code>
					</p>
					<div class="form-grid">
						<label class="form-field">
							<span class="form-label">Name</span>
							<Input bind:value={basicName} placeholder="Project name" />
						</label>
						<label class="form-field">
							<span class="form-label">Extensions (comma separated)</span>
							<Input bind:value={basicExtensions} placeholder="rs, py, ts" />
						</label>
						<label class="form-field">
							<span class="form-label">Exclude dirs (comma separated)</span>
							<Input bind:value={basicExcludeDirs} placeholder="target, node_modules" />
						</label>
						<label class="form-field">
							<span class="form-label">Ignore patterns (comma separated)</span>
							<Input bind:value={basicIgnorePatterns} placeholder="*.log, dist" />
						</label>
					</div>
					<div class="toggle-row">
						<Toggle
							checked={basicRespectGitignore}
							label="Respect .gitignore"
							onchange={(e) => (basicRespectGitignore = e.checked)}
						/>
					</div>

					<div class="reload-actions">
						<Button onclick={saveProjectBasics} disabled={basicSaving || basicLoading}>
							{#if basicSaving}Saving...{:else}Save Basics{/if}
						</Button>
					</div>

					{#if basicError}
						<div class="editor-error">{basicError}</div>
					{/if}
					{#if basicResult}
						<div class="reload-result">
							<Badge label="Saved" variant="active" />
							<span class="reload-message">{basicResult}</span>
						</div>
					{/if}
				{/if}
			</Card>

			<Card
				title="Project Runtime Config"
				subtitle="Merged global defaults plus project overrides (full document)"
			>
				{#if runtimeLoading && !runtimeLoaded}
					<p class="placeholder-text">Loading runtime config...</p>
				{:else}
					<div class="version-row">
						<span class="config-label">Project #{ $currentProjectId }</span>
						{#if runtimeVersion !== null}
							<Badge label={`v${runtimeVersion}`} variant="active" />
						{/if}
						{#if validateResult}
							<Badge
								label={validateResult.valid ? 'Global Valid' : 'Global Invalid'}
								variant={validateResult.valid ? 'active' : 'inactive'}
							/>
						{/if}
						<span class="validate-spacer"></span>
						<Button onclick={loadValidate} disabled={validating}>
							{#if validating}Checking...{:else}Re-validate{/if}
						</Button>
					</div>
					<textarea
						class="config-editor"
						bind:value={runtimeText}
						spellcheck="false"
						aria-label="Project runtime configuration JSON"></textarea>

					<div class="reload-actions split">
						<Button
							onclick={handleReloadFromFile}
							disabled={reloading || runtimeLoading}
							variant="secondary"
						>
							{#if reloading}Reloading...{:else}Reload From File{/if}
						</Button>
						<Button
							onclick={saveProjectRuntime}
							disabled={runtimeSaving || runtimeLoading}
						>
							{#if runtimeSaving}Saving...{:else}Save Runtime Config{/if}
						</Button>
					</div>

					{#if runtimeError}
						<div class="editor-error">{runtimeError}</div>
					{/if}

					{#if runtimeResult}
						<div class="reload-result">
							<Badge
								label={runtimeResult.success ? 'Success' : 'Failed'}
								variant={runtimeResult.success ? 'active' : 'inactive'}
							/>
							<Badge
								label={runtimeResult.hot_reload_applied
									? 'Hot Reload'
									: 'Restart Needed'}
								variant={runtimeResult.hot_reload_applied
									? 'success'
									: 'warning'}
							/>
							<span class="reload-message">{runtimeResult.message}</span>
						</div>
					{/if}

					{#if reloadMessage}
						<div class="reload-result">
							<Badge label="Reloaded" variant="active" />
							<span class="reload-message">{reloadMessage}</span>
						</div>
					{/if}
				{/if}
			</Card>
		{/if}
	</div>
</div>

<style>
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

	/* Scope Navigation */
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
		max-height: 480px;
		overflow-y: auto;
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

	.validate-spacer {
		flex: 1;
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

	.reload-actions.split {
		justify-content: space-between;
	}

	.reload-result {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem;
		border: 1px solid var(--gray-200);
		margin-top: 1rem;
	}

	.reload-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.version-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.form-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.form-field {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.form-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.toggle-row {
		margin: 0.5rem 0 1rem;
	}

	.config-editor {
		width: 100%;
		min-height: 320px;
		padding: 1rem;
		background: var(--gray-50);
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		line-height: 1.6;
		color: var(--black);
		resize: vertical;
	}

	.config-editor:focus {
		outline: none;
		border-color: var(--accent);
	}

	.editor-error {
		margin-top: 1rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		background: var(--danger-bg, #fff5f5);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
</style>
