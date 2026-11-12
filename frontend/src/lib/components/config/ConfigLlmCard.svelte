<script lang="ts">
	/**
	 * Project LLM card: masked provider/model overview with dimension
	 * diagnostics, typed chat/rerank editing, connectivity probes and
	 * reindex guidance. Page owns all requests; this card renders sections
	 * and reports actions through callbacks.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import type {
		ProjectLlmOverviewResponse,
		ProjectLlmUpdateResponse,
		LlmTestResponse,
	} from '$lib/api/config';

	interface Props {
		overview?: ProjectLlmOverviewResponse | null;
		loaded?: boolean;
		loading?: boolean;
		saving?: boolean;
		loadError?: string | null;
		saveError?: string | null;
		saveResult?: ProjectLlmUpdateResponse | null;
		defaultChat?: string;
		defaultRerank?: string;
		rerankEnabled?: boolean;
		rerankMaxCandidates?: string;
		chatTemperature?: string;
		chatMaxTokens?: string;
		chatTopP?: string;
		chatMaxInputTokens?: string;
		testing?: boolean;
		testResult?: LlmTestResponse | null;
		testError?: string | null;
		reindexBusy?: boolean;
		reindexMessage?: string | null;
		projectId?: string | number | null;
		onReload: () => void;
		onSave: () => void;
		onTest: (service: string, modelKey: string) => void;
		onClearVectors: () => void;
		onReindex: () => void;
	}

	let {
		overview = null,
		loaded = false,
		loading = false,
		saving = false,
		loadError = null,
		saveError = null,
		saveResult = null,
		defaultChat = $bindable(''),
		defaultRerank = $bindable(''),
		rerankEnabled = $bindable(true),
		rerankMaxCandidates = $bindable(''),
		chatTemperature = $bindable(''),
		chatMaxTokens = $bindable(''),
		chatTopP = $bindable(''),
		chatMaxInputTokens = $bindable(''),
		testing = false,
		testResult = null,
		testError = null,
		reindexBusy = false,
		reindexMessage = null,
		projectId = null,
		onReload,
		onSave,
		onTest,
		onClearVectors,
		onReindex,
	}: Props = $props();
</script>

<Card
	title="Project LLM Models"
	subtitle="Masked overview; chat/rerank edits apply in memory"
>
	{#if loading && !loaded}
		<p class="empty-hint">Loading model overview...</p>
	{:else if loadError && !overview}
		<InlineError message={loadError} />
		<div class="reload-actions">
			<Button onclick={onReload} disabled={loading} variant="secondary"
				>Retry</Button
			>
		</div>
	{:else if overview}
		<div class="version-row">
			<span class="form-label">Project #{projectId}</span>
			{#if overview.config_version !== undefined}
				<Badge label={`v${overview.config_version}`} variant="active" />
			{/if}
			<Badge
				label={overview.llm_enabled ? 'LLM Enabled' : 'LLM Disabled'}
				variant={overview.llm_enabled ? 'active' : 'inactive'}
			/>
			<Badge
				label={overview.dimension.matches
					? 'Dimension OK'
					: 'Dimension Mismatch'}
				variant={overview.dimension.matches ? 'success' : 'danger'}
			/>
			<span style="flex: 1"></span>
			<Button onclick={onReload} disabled={loading} variant="secondary">
				{#if loading}Refreshing...{:else}Refresh{/if}
			</Button>
		</div>

		{#if overview.ephemeral_note}
			<p class="reload-description">{overview.ephemeral_note}</p>
		{/if}

		{#if !overview.dimension.matches}
			<InlineError>
				{overview.dimension.message} (model {overview.dimension
					.default_embedding_dimension ?? '?'}
				vs backend {overview.dimension.effective_vector_size}). Embedding
				selection is global: edit config.toml and restart, then reindex.
			</InlineError>
			<div class="reload-actions reload-actions--split">
				<Button
					onclick={onClearVectors}
					disabled={reindexBusy}
					variant="secondary"
				>
					{#if reindexBusy}Working...{:else}Clear Vectors{/if}
				</Button>
				<Button onclick={onReindex} disabled={reindexBusy}>
					{#if reindexBusy}Working...{:else}Reindex Project{/if}
				</Button>
			</div>
			{#if reindexMessage}
				<div class="reload-result">
					<span class="reload-message">{reindexMessage}</span>
				</div>
			{/if}
		{/if}

		{#if overview.missing_api_key_providers.length > 0}
			<div class="reload-result">
				<Badge label="Keys Missing" variant="warning" />
				<span class="reload-message">
					No API keys for: {overview.missing_api_key_providers.join(', ')}. Set
					env placeholders or key files on the server host.
				</span>
			</div>
		{/if}

		{#if overview.unknown_provider_refs.length > 0}
			<InlineError>
				Unknown provider refs: {overview.unknown_provider_refs.join('; ')}
			</InlineError>
		{/if}

		<h3 class="section-title">Providers (masked, read-only)</h3>
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr
						><th>ID</th><th>Name</th><th>Base URL</th><th>Keys</th><th
							>Models</th
						></tr
					>
				</thead>
				<tbody>
					{#each overview.providers as provider (provider.id)}
						<tr>
							<td><code>{provider.id}</code></td>
							<td>{provider.name}</td>
							<td class="font-mono">{provider.base_url}</td>
							<td>
								{#if provider.has_api_keys}
									<Badge label={`${provider.key_count} set`} variant="active" />
								{:else}
									<Badge label="none" variant="inactive" />
								{/if}
								{#if provider.has_unresolved_placeholder}
									<Badge label="placeholder?" variant="warning" />
								{/if}
							</td>
							<td class="font-mono">
								E:{provider.embedding_model_count} C:{provider.chat_model_count} R:{provider.rerank_model_count}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<h3 class="section-title">
			Embedding models (global-effective, read-only)
		</h3>
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr
						><th>Key</th><th>Model</th><th>Dim</th><th>Default</th><th>Probe</th
						></tr
					>
				</thead>
				<tbody>
					{#each overview.embedding_models as model (model.key)}
						<tr>
							<td><code>{model.key}</code></td>
							<td class="font-mono">{model.model}</td>
							<td>{model.vector_dimension}</td>
							<td>
								{#if model.is_default}<Badge
										label="default"
										variant="active"
									/>{/if}
							</td>
							<td>
								<Button
									onclick={() => onTest('embedding', model.key)}
									disabled={testing}
									variant="secondary"
									size="sm">Test</Button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<h3 class="section-title">
			Chat defaults and parameters (project-effective)
		</h3>
		<div class="form-grid">
			<label class="form-field">
				<span class="form-label">Default chat model</span>
				<select
					bind:value={defaultChat}
					class="text-input"
					aria-label="Default chat model"
				>
					<option value="">-- unset --</option>
					{#each overview.chat_models as model (model.key)}
						<option value={model.key}>{model.key} ({model.model})</option>
					{/each}
				</select>
			</label>
			<label class="form-field">
				<span class="form-label">Default rerank model</span>
				<select
					bind:value={defaultRerank}
					class="text-input"
					aria-label="Default rerank model"
				>
					<option value="">-- unset --</option>
					{#each overview.rerank_models as model (model.key)}
						<option value={model.key}>{model.key} ({model.model})</option>
					{/each}
				</select>
			</label>
		</div>
		<div class="form-grid">
			<label class="form-field">
				<span class="form-label">Temperature (selected chat model)</span>
				<Input bind:value={chatTemperature} placeholder="e.g. 0.3" />
			</label>
			<label class="form-field">
				<span class="form-label">Max tokens</span>
				<Input bind:value={chatMaxTokens} placeholder="e.g. 2048" />
			</label>
			<label class="form-field">
				<span class="form-label">Top P</span>
				<Input bind:value={chatTopP} placeholder="e.g. 1.0" />
			</label>
			<label class="form-field">
				<span class="form-label">Max input tokens</span>
				<Input bind:value={chatMaxInputTokens} placeholder="e.g. 8192" />
			</label>
		</div>
		<div class="form-grid">
			<Toggle
				checked={rerankEnabled}
				label="Rerank enabled"
				onchange={(e) => (rerankEnabled = e.checked)}
			/>
			<label class="form-field">
				<span class="form-label">Rerank max candidates</span>
				<Input bind:value={rerankMaxCandidates} placeholder="e.g. 50" />
			</label>
		</div>

		<h3 class="section-title">Chat models</h3>
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr
						><th>Key</th><th>Model</th><th>Temp</th><th>Default</th><th
							>Probe</th
						></tr
					>
				</thead>
				<tbody>
					{#each overview.chat_models as model (model.key)}
						<tr>
							<td><code>{model.key}</code></td>
							<td class="font-mono">{model.model}</td>
							<td>{model.temperature}</td>
							<td>
								{#if model.is_default}<Badge
										label="default"
										variant="active"
									/>{/if}
							</td>
							<td>
								<Button
									onclick={() => onTest('chat', model.key)}
									disabled={testing}
									variant="secondary"
									size="sm">Test</Button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		{#if testing}
			<p class="empty-hint">Probing model...</p>
		{/if}
		{#if testError}
			<InlineError message={testError} />
		{/if}
		{#if testResult}
			<div class="reload-result">
				<Badge
					label={testResult.success ? 'Probe OK' : 'Probe Failed'}
					variant={testResult.success ? 'success' : 'danger'}
				/>
				<span class="reload-message">
					{testResult.service}:{testResult.model_key} in {testResult.latency_ms}ms
					—
					{testResult.message}
				</span>
			</div>
		{/if}

		<div class="reload-actions reload-actions--split">
			<span class="form-label">Edits stay in memory until reload/restart</span
			>
			<Button onclick={onSave} disabled={saving || loading}>
				{#if saving}Saving...{:else}Save Chat/Rerank Config{/if}
			</Button>
		</div>
		{#if saveError}
			<InlineError message={saveError} />
		{/if}
		{#if saveResult}
			<div class="reload-result">
				<Badge
					label={saveResult.success ? 'Success' : 'Failed'}
					variant={saveResult.success ? 'active' : 'inactive'}
				/>
				<Badge
					label={saveResult.hot_reload_applied
						? 'Hot Reload'
						: 'Restart Needed'}
					variant={saveResult.hot_reload_applied ? 'success' : 'warning'}
				/>
				<Badge label="Ephemeral" variant="info" />
				<span class="reload-message"
					>v{saveResult.config_version} — {saveResult.message}</span
				>
			</div>
		{/if}
	{:else}
		<p class="empty-hint">
			No model data. Open a project to load its merged LLM view.
		</p>
	{/if}
</Card>

<style>
</style>
