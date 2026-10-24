<script lang="ts">
	/**
	 * Project LLM card: masked provider/model overview with dimension
	 * diagnostics, typed chat/rerank editing, connectivity probes and
	 * reindex guidance. Page owns all requests; this card renders sections
	 * and reports actions through callbacks.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
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
		<p class="placeholder-text">Loading model overview...</p>
	{:else if loadError && !overview}
		<div class="editor-error">{loadError}</div>
		<div class="reload-actions">
			<Button onclick={onReload} disabled={loading} variant="secondary">Retry</Button>
		</div>
	{:else if overview}
		<div class="version-row">
			<span class="config-label">Project #{projectId}</span>
			{#if overview.config_version !== undefined}
				<Badge label={`v${overview.config_version}`} variant="active" />
			{/if}
			<Badge
				label={overview.llm_enabled ? 'LLM Enabled' : 'LLM Disabled'}
				variant={overview.llm_enabled ? 'active' : 'inactive'}
			/>
			<Badge
				label={overview.dimension.matches ? 'Dimension OK' : 'Dimension Mismatch'}
				variant={overview.dimension.matches ? 'success' : 'danger'}
			/>
			<span class="validate-spacer"></span>
			<Button onclick={onReload} disabled={loading} variant="secondary">
				{#if loading}Refreshing...{:else}Refresh{/if}
			</Button>
		</div>

		{#if overview.ephemeral_note}
			<p class="reload-description">{overview.ephemeral_note}</p>
		{/if}

		{#if !overview.dimension.matches}
			<div class="editor-error">
				{overview.dimension.message} (model {overview.dimension.default_embedding_dimension ?? '?'}
				vs backend {overview.dimension.effective_vector_size}).
				Embedding selection is global: edit config.toml and restart, then reindex.
			</div>
			<div class="reload-actions split">
				<Button onclick={onClearVectors} disabled={reindexBusy} variant="secondary">
					{#if reindexBusy}Working...{:else}Clear Vectors{/if}
				</Button>
				<Button onclick={onReindex} disabled={reindexBusy}>
					{#if reindexBusy}Working...{:else}Reindex Project{/if}
				</Button>
			</div>
			{#if reindexMessage}
				<div class="reload-result"><span class="reload-message">{reindexMessage}</span></div>
			{/if}
		{/if}

		{#if overview.missing_api_key_providers.length > 0}
			<div class="reload-result">
				<Badge label="Keys Missing" variant="warning" />
				<span class="reload-message">
					No API keys for: {overview.missing_api_key_providers.join(', ')}.
					Set env placeholders or key files on the server host.
				</span>
			</div>
		{/if}

		{#if overview.unknown_provider_refs.length > 0}
			<div class="editor-error">
				Unknown provider refs: {overview.unknown_provider_refs.join('; ')}
			</div>
		{/if}

		<h3 class="section-title">Providers (masked, read-only)</h3>
		<div class="table-wrap">
			<table class="model-table">
				<thead>
					<tr><th>ID</th><th>Name</th><th>Base URL</th><th>Keys</th><th>Models</th></tr>
				</thead>
				<tbody>
					{#each overview.providers as provider (provider.id)}
						<tr>
							<td><code>{provider.id}</code></td>
							<td>{provider.name}</td>
							<td class="mono">{provider.base_url}</td>
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
							<td class="mono">
								E:{provider.embedding_model_count} C:{provider.chat_model_count} R:{provider.rerank_model_count}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<h3 class="section-title">Embedding models (global-effective, read-only)</h3>
		<div class="table-wrap">
			<table class="model-table">
				<thead>
					<tr><th>Key</th><th>Model</th><th>Dim</th><th>Default</th><th>Probe</th></tr>
				</thead>
				<tbody>
					{#each overview.embedding_models as model (model.key)}
						<tr>
							<td><code>{model.key}</code></td>
							<td class="mono">{model.model}</td>
							<td>{model.vector_dimension}</td>
							<td>
								{#if model.is_default}<Badge label="default" variant="active" />{/if}
							</td>
							<td>
								<Button
									onclick={() => onTest('embedding', model.key)}
									disabled={testing}
									variant="secondary"
									size="sm"
								>Test</Button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<h3 class="section-title">Chat defaults and parameters (project-effective)</h3>
		<div class="form-grid">
			<label class="field">
				<span class="config-label">Default chat model</span>
				<select bind:value={defaultChat} class="select-input" aria-label="Default chat model">
					<option value="">-- unset --</option>
					{#each overview.chat_models as model (model.key)}
						<option value={model.key}>{model.key} ({model.model})</option>
					{/each}
				</select>
			</label>
			<label class="field">
				<span class="config-label">Default rerank model</span>
				<select bind:value={defaultRerank} class="select-input" aria-label="Default rerank model">
					<option value="">-- unset --</option>
					{#each overview.rerank_models as model (model.key)}
						<option value={model.key}>{model.key} ({model.model})</option>
					{/each}
				</select>
			</label>
		</div>
		<div class="form-grid">
			<label class="field">
				<span class="config-label">Temperature (selected chat model)</span>
				<Input bind:value={chatTemperature} placeholder="e.g. 0.3" />
			</label>
			<label class="field">
				<span class="config-label">Max tokens</span>
				<Input bind:value={chatMaxTokens} placeholder="e.g. 2048" />
			</label>
			<label class="field">
				<span class="config-label">Top P</span>
				<Input bind:value={chatTopP} placeholder="e.g. 1.0" />
			</label>
			<label class="field">
				<span class="config-label">Max input tokens</span>
				<Input bind:value={chatMaxInputTokens} placeholder="e.g. 8192" />
			</label>
		</div>
		<div class="form-grid">
			<Toggle
				checked={rerankEnabled}
				label="Rerank enabled"
				onchange={(e) => (rerankEnabled = e.checked)}
			/>
			<label class="field">
				<span class="config-label">Rerank max candidates</span>
				<Input bind:value={rerankMaxCandidates} placeholder="e.g. 50" />
			</label>
		</div>

		<h3 class="section-title">Chat models</h3>
		<div class="table-wrap">
			<table class="model-table">
				<thead>
					<tr><th>Key</th><th>Model</th><th>Temp</th><th>Default</th><th>Probe</th></tr>
				</thead>
				<tbody>
					{#each overview.chat_models as model (model.key)}
						<tr>
							<td><code>{model.key}</code></td>
							<td class="mono">{model.model}</td>
							<td>{model.temperature}</td>
							<td>
								{#if model.is_default}<Badge label="default" variant="active" />{/if}
							</td>
							<td>
								<Button
									onclick={() => onTest('chat', model.key)}
									disabled={testing}
									variant="secondary"
									size="sm"
								>Test</Button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		{#if testing}
			<p class="placeholder-text">Probing model...</p>
		{/if}
		{#if testError}
			<div class="editor-error">{testError}</div>
		{/if}
		{#if testResult}
			<div class="reload-result">
				<Badge
					label={testResult.success ? 'Probe OK' : 'Probe Failed'}
					variant={testResult.success ? 'success' : 'danger'}
				/>
				<span class="reload-message">
					{testResult.service}:{testResult.model_key} in {testResult.latency_ms}ms —
					{testResult.message}
				</span>
			</div>
		{/if}

		<div class="reload-actions split">
			<span class="config-label">Edits stay in memory until reload/restart</span>
			<Button onclick={onSave} disabled={saving || loading}>
				{#if saving}Saving...{:else}Save Chat/Rerank Config{/if}
			</Button>
		</div>
		{#if saveError}
			<div class="editor-error">{saveError}</div>
		{/if}
		{#if saveResult}
			<div class="reload-result">
				<Badge label={saveResult.success ? 'Success' : 'Failed'} variant={saveResult.success ? 'active' : 'inactive'} />
				<Badge
					label={saveResult.hot_reload_applied ? 'Hot Reload' : 'Restart Needed'}
					variant={saveResult.hot_reload_applied ? 'success' : 'warning'}
				/>
				<Badge label="Ephemeral" variant="info" />
				<span class="reload-message">v{saveResult.config_version} — {saveResult.message}</span>
			</div>
		{/if}
	{:else}
		<p class="placeholder-text">No model data. Open a project to load its merged LLM view.</p>
	{/if}
</Card>

<style>
	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 1.5rem;
	}
	.version-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
		flex-wrap: wrap;
	}
	.config-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}
	.validate-spacer {
		flex: 1;
	}
	.reload-description {
		color: var(--gray-600);
		margin-bottom: 1rem;
		line-height: 1.6;
		font-size: 0.85rem;
	}
	.section-title {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		font-weight: 700;
		margin: 1.5rem 0 0.75rem;
	}
	.table-wrap {
		overflow-x: auto;
		border: 1px solid var(--gray-200);
	}
	.model-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8rem;
	}
	.model-table th,
	.model-table td {
		text-align: left;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--gray-200);
	}
	.model-table th {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
		background: var(--gray-50);
	}
	.mono {
		font-family: 'Space Mono', monospace;
	}
	.form-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 0.75rem;
		margin: 0.75rem 0;
		align-items: end;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.select-input {
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}
	.reload-actions {
		display: flex;
		justify-content: flex-end;
		margin: 1rem 0;
	}
	.reload-actions.split {
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.75rem;
	}
	.reload-result {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--gray-200);
		margin-top: 0.75rem;
		flex-wrap: wrap;
	}
	.reload-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}
	.editor-error {
		margin-top: 0.75rem;
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
