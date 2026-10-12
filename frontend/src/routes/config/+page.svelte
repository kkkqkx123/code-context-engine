<script lang="ts">
	/**
	 * Configuration page.
	 *
	 * Scope-based navigation: global state is read-only, project state is
	 * editable. The page owns all request orchestration and form state so
	 * unsaved edits survive tab switches; each card component renders one
	 * section and reports actions back through callbacks.
	 */
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { onMount } from 'svelte';
	import ConfigTokenCard from '$lib/components/config/ConfigTokenCard.svelte';
	import ConfigGlobalCard from '$lib/components/config/ConfigGlobalCard.svelte';
	import ConfigValidationCard from '$lib/components/config/ConfigValidationCard.svelte';
	import ConfigProjectBasicsCard from '$lib/components/config/ConfigProjectBasicsCard.svelte';
	import ConfigRuntimeCard from '$lib/components/config/ConfigRuntimeCard.svelte';
	import {
		configApi,
		type ConfigInfoResponse,
		type ConfigValidateResponse,
	} from '$lib/api/config';
	import { projectApi, type ProjectConfigUpdateResponse } from '$lib/api';
	import { currentProjectId, onProjectChange } from '$lib/stores/project';
	import {
		apiToken,
		setApiToken,
		clearApiToken,
	} from '$lib/stores/auth';
	import { get } from 'svelte/store';
	import { errorMessage } from '$lib/utils/errors';
	import { parseList } from '$lib/utils/text';

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

	// ─── Remote access token (browser-local admission credential) ──
	let tokenDraft = $state('');
	let tokenMessage = $state<string | null>(null);
	let tokenSaving = $state(false);

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

	async function saveAccessToken() {
		tokenMessage = null;
		tokenSaving = true;
		try {
			setApiToken(tokenDraft);
			tokenDraft = get(apiToken);
			await loadInfo();
			tokenMessage = get(apiToken)
				? 'Access token saved; requests now carry it.'
				: 'Access token cleared; requests go token-free.';
		} catch (e) {
			tokenMessage = `Token saved locally, but the server rejected the check: ${errorMessage(e)}`;
		} finally {
			tokenSaving = false;
		}
	}

	function clearAccessToken() {
		tokenDraft = '';
		tokenMessage = null;
		clearApiToken();
		void loadInfo();
	}

	onMount(async () => {
		tokenDraft = get(apiToken);
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
			<ConfigTokenCard
				bind:tokenDraft
				{tokenSaving}
				{tokenMessage}
				onSave={saveAccessToken}
				onClear={clearAccessToken}
			/>

			<ConfigGlobalCard {configInfo} {loading} onRefresh={loadInfo} />

			<ConfigValidationCard {validateResult} {validating} onValidate={loadValidate} />
		{/if}

		{#if activeScope === 'project'}
			<ConfigProjectBasicsCard
				{rootPath}
				bind:basicName
				bind:basicExtensions
				bind:basicExcludeDirs
				bind:basicIgnorePatterns
				bind:basicRespectGitignore
				{basicLoaded}
				{basicLoading}
				{basicSaving}
				{basicError}
				{basicResult}
				onSave={saveProjectBasics}
			/>

			<ConfigRuntimeCard
				bind:runtimeText
				{runtimeLoaded}
				{runtimeLoading}
				{runtimeSaving}
				{runtimeError}
				{runtimeResult}
				{runtimeVersion}
				{reloadMessage}
				{reloading}
				{validateResult}
				{validating}
				projectId={$currentProjectId}
				onValidate={loadValidate}
				onReloadFromFile={handleReloadFromFile}
				onSave={saveProjectRuntime}
			/>
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
</style>
