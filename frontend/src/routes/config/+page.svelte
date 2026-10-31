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
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
	import { onMount } from 'svelte';
	import ConfigTokenCard from '$lib/components/config/ConfigTokenCard.svelte';
	import ConfigGlobalCard from '$lib/components/config/ConfigGlobalCard.svelte';
	import ConfigValidationCard from '$lib/components/config/ConfigValidationCard.svelte';
	import ConfigProjectBasicsCard from '$lib/components/config/ConfigProjectBasicsCard.svelte';
	import ConfigRuntimeCard from '$lib/components/config/ConfigRuntimeCard.svelte';
	import ConfigLlmCard from '$lib/components/config/ConfigLlmCard.svelte';
	import {
		configApi,
		llmApi,
		type ConfigInfoResponse,
		type ConfigValidateResponse,
		type ProjectLlmOverviewResponse,
		type ProjectLlmUpdateResponse,
		type LlmTestResponse,
	} from '$lib/api/config';
	import {
		projectApi,
		indexApi,
		type ProjectConfigUpdateResponse,
	} from '$lib/api';
	import { projectStore, onProjectChange } from '$lib/stores/project.svelte';
	import { authStore } from '$lib/stores/auth.svelte';
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

	// ─── Project LLM overview + typed edit + probe (via /llm/*) ──
	let llmOverview = $state<ProjectLlmOverviewResponse | null>(null);
	let llmLoaded = $state(false);
	let llmLoading = $state(false);
	let llmLoadError = $state<string | null>(null);
	let llmSaving = $state(false);
	let llmSaveError = $state<string | null>(null);
	let llmSaveResult = $state<ProjectLlmUpdateResponse | null>(null);
	let llmDefaultChat = $state('');
	let llmDefaultRerank = $state('');
	let llmRerankEnabled = $state(true);
	let llmRerankMaxCandidates = $state('');
	let llmChatTemperature = $state('');
	let llmChatMaxTokens = $state('');
	let llmChatTopP = $state('');
	let llmChatMaxInputTokens = $state('');
	let llmTesting = $state(false);
	let llmTestResult = $state<LlmTestResponse | null>(null);
	let llmTestError = $state<string | null>(null);
	let reindexBusy = $state(false);
	let reindexMessage = $state<string | null>(null);

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
			const detail = await projectApi.getProject(
				String(projectStore.currentProjectId),
			);
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
				String(projectStore.currentProjectId),
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
		if (!llmLoaded && !llmLoading) void loadLlmOverview();
	}

	async function loadLlmOverview() {
		llmLoading = true;
		llmLoadError = null;
		try {
			const response = await llmApi.getOverview(
				String(projectStore.currentProjectId),
			);
			llmOverview = response;
			llmLoaded = true;
			llmDefaultChat = response.default_chat ?? '';
			llmDefaultRerank = response.default_rerank ?? '';
			llmRerankEnabled = response.rerank_enabled;
			llmRerankMaxCandidates = String(response.rerank_max_candidates ?? '');
			const activeChat = response.chat_models.find(
				(model) => model.key === (response.default_chat ?? ''),
			);
			llmChatTemperature = activeChat ? String(activeChat.temperature) : '';
			llmChatMaxTokens = activeChat ? String(activeChat.max_tokens) : '';
			llmChatTopP = activeChat ? String(activeChat.top_p) : '';
			llmChatMaxInputTokens = activeChat
				? String(activeChat.max_input_tokens)
				: '';
			llmSaveResult = null;
			llmTestResult = null;
			llmTestError = null;
		} catch (e) {
			llmLoadError = errorMessage(e);
		} finally {
			llmLoading = false;
		}
	}

	function parseOptionalNumber(text: string): number | undefined {
		const trimmed = text.trim();
		if (!trimmed) return undefined;
		const value = Number(trimmed);
		return Number.isFinite(value) ? value : undefined;
	}

	async function saveLlmConfig() {
		llmSaveError = null;
		llmSaveResult = null;
		const trimmedChat = llmDefaultChat.trim();
		const trimmedRerank = llmDefaultRerank.trim();
		const maxCandidates = parseOptionalNumber(llmRerankMaxCandidates);
		if (llmRerankMaxCandidates.trim() && maxCandidates === undefined) {
			llmSaveError = 'Rerank max candidates must be a number';
			return;
		}
		const temperature = llmChatTemperature.trim()
			? Number(llmChatTemperature.trim())
			: undefined;
		const maxTokens = parseOptionalNumber(llmChatMaxTokens);
		const topP = llmChatTopP.trim() ? Number(llmChatTopP.trim()) : undefined;
		const maxInputTokens = parseOptionalNumber(llmChatMaxInputTokens);
		if (
			llmChatTemperature.trim() &&
			(temperature === undefined || !Number.isFinite(temperature))
		) {
			llmSaveError = 'Temperature must be a number';
			return;
		}
		if (llmChatMaxTokens.trim() && maxTokens === undefined) {
			llmSaveError = 'Max tokens must be a number';
			return;
		}
		if (llmChatTopP.trim() && (topP === undefined || !Number.isFinite(topP))) {
			llmSaveError = 'Top P must be a number';
			return;
		}
		if (llmChatMaxInputTokens.trim() && maxInputTokens === undefined) {
			llmSaveError = 'Max input tokens must be a number';
			return;
		}
		const chatModelParams =
			trimmedChat &&
			(temperature !== undefined ||
				maxTokens !== undefined ||
				topP !== undefined ||
				maxInputTokens !== undefined)
				? {
						[trimmedChat]: {
							...(temperature !== undefined ? { temperature } : {}),
							...(maxTokens !== undefined ? { max_tokens: maxTokens } : {}),
							...(topP !== undefined ? { top_p: topP } : {}),
							...(maxInputTokens !== undefined
								? { max_input_tokens: maxInputTokens }
								: {}),
						},
					}
				: undefined;
		llmSaving = true;
		try {
			llmSaveResult = await llmApi.updateConfig(
				String(projectStore.currentProjectId),
				{
					default_chat: trimmedChat ? trimmedChat : null,
					default_rerank: trimmedRerank ? trimmedRerank : null,
					rerank_enabled: llmRerankEnabled,
					rerank_max_candidates: maxCandidates ?? null,
					chat_model_params: chatModelParams ?? null,
				},
			);
			void loadLlmOverview();
			void loadValidate();
		} catch (e) {
			llmSaveError = errorMessage(e);
		} finally {
			llmSaving = false;
		}
	}

	async function testLlmModel(service: string, modelKey: string) {
		llmTesting = true;
		llmTestError = null;
		llmTestResult = null;
		try {
			llmTestResult = await llmApi.testModel(
				String(projectStore.currentProjectId),
				{
					service,
					model_key: modelKey,
				},
			);
		} catch (e) {
			llmTestError = errorMessage(e);
		} finally {
			llmTesting = false;
		}
	}

	async function clearProjectVectors() {
		reindexBusy = true;
		reindexMessage = null;
		try {
			const response = await indexApi.clearIndex(
				Number(projectStore.currentProjectId),
			);
			reindexMessage =
				response.message ?? `Clear finished (success=${response.success})`;
		} catch (e) {
			reindexMessage = errorMessage(e);
		} finally {
			reindexBusy = false;
		}
	}

	async function reindexProject() {
		reindexBusy = true;
		reindexMessage = null;
		try {
			const response = await projectApi.indexProject(
				String(projectStore.currentProjectId),
			);
			reindexMessage = `Reindex started for project ${response.project_id ?? projectStore.currentProjectId}`;
		} catch (e) {
			reindexMessage = errorMessage(e);
		} finally {
			reindexBusy = false;
		}
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
			await projectApi.updateProject(String(projectStore.currentProjectId), {
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
				String(projectStore.currentProjectId),
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
				String(projectStore.currentProjectId),
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
				String(projectStore.currentProjectId),
			);
			runtimeVersion = response.config_version;
			reloadMessage = response.message;
			// Reloading from disk can change both panes, refresh them together.
			void loadProjectBasics();
			void loadProjectRuntime();
			void loadLlmOverview();
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
			authStore.setApiToken(tokenDraft);
			tokenDraft = authStore.token;
			await loadInfo();
			tokenMessage = authStore.token
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
		authStore.clearApiToken();
		void loadInfo();
	}

	onMount(async () => {
		tokenDraft = authStore.token;
		await loadInfo();
		await loadValidate();
	});

	// Both project panes track one project; reload them after a switch.
	$effect(() =>
		onProjectChange(() => {
			if (basicLoaded) void loadProjectBasics();
			if (runtimeLoaded) void loadProjectRuntime();
			if (llmLoaded) void loadLlmOverview();
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
			<ErrorBanner message={error} onDismiss={() => (error = null)} />
		{/if}

		<Tabs
			variant="underline"
			label="Configuration scope"
			active={activeScope}
			tabs={[
				{ id: 'global', label: 'Global' },
				{ id: 'project', label: `Project #${projectStore.currentProjectId}` },
			]}
			onSelect={(id) => {
				activeScope = id as typeof activeScope;
				if (id === 'project') ensureProjectLoaded();
			}}
		/>

		{#if activeScope === 'global'}
			<ConfigTokenCard
				bind:tokenDraft
				{tokenSaving}
				{tokenMessage}
				onSave={saveAccessToken}
				onClear={clearAccessToken}
			/>

			<ConfigGlobalCard {configInfo} {loading} onRefresh={loadInfo} />

			<ConfigValidationCard
				{validateResult}
				{validating}
				onValidate={loadValidate}
			/>
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

			<ConfigLlmCard
				overview={llmOverview}
				loaded={llmLoaded}
				loading={llmLoading}
				saving={llmSaving}
				loadError={llmLoadError}
				saveError={llmSaveError}
				saveResult={llmSaveResult}
				bind:defaultChat={llmDefaultChat}
				bind:defaultRerank={llmDefaultRerank}
				bind:rerankEnabled={llmRerankEnabled}
				bind:rerankMaxCandidates={llmRerankMaxCandidates}
				bind:chatTemperature={llmChatTemperature}
				bind:chatMaxTokens={llmChatMaxTokens}
				bind:chatTopP={llmChatTopP}
				bind:chatMaxInputTokens={llmChatMaxInputTokens}
				testing={llmTesting}
				testResult={llmTestResult}
				testError={llmTestError}
				{reindexBusy}
				{reindexMessage}
				projectId={projectStore.currentProjectId}
				onReload={() => void loadLlmOverview()}
				onSave={() => void saveLlmConfig()}
				onTest={(service, modelKey) => void testLlmModel(service, modelKey)}
				onClearVectors={() => void clearProjectVectors()}
				onReindex={() => void reindexProject()}
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
				projectId={projectStore.currentProjectId}
				onValidate={loadValidate}
				onReloadFromFile={handleReloadFromFile}
				onSave={saveProjectRuntime}
			/>
		{/if}
	</div>
</div>

<style>
</style>
