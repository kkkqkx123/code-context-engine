<script lang="ts">
	/**
	 * Project runtime config card for the config page: JSON editor over the
	 * merged AppConfig document, with version stamp, reload-from-file and
	 * save actions. Editor state is bound from the page so unsaved edits
	 * survive tab switches; the page owns every request.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import type { ConfigValidateResponse } from '$lib/api/config';
	import type { ProjectConfigUpdateResponse } from '$lib/api';

	interface Props {
		runtimeText?: string;
		runtimeLoaded?: boolean;
		runtimeLoading?: boolean;
		runtimeSaving?: boolean;
		runtimeError?: string | null;
		runtimeResult?: ProjectConfigUpdateResponse | null;
		runtimeVersion?: number | null;
		reloadMessage?: string | null;
		reloading?: boolean;
		validateResult?: ConfigValidateResponse | null;
		validating?: boolean;
		/** Current project id, shown in the version row. */
		projectId?: string | number | null;
		onValidate: () => void;
		onReloadFromFile: () => void;
		onSave: () => void;
	}

	let {
		runtimeText = $bindable(''),
		runtimeLoaded = false,
		runtimeLoading = false,
		runtimeSaving = false,
		runtimeError = null,
		runtimeResult = null,
		runtimeVersion = null,
		reloadMessage = null,
		reloading = false,
		validateResult = null,
		validating = false,
		projectId = null,
		onValidate,
		onReloadFromFile,
		onSave,
	}: Props = $props();
</script>

<Card
	title="Project Runtime Config"
	subtitle="Merged global defaults plus project overrides (full document)"
>
	{#if runtimeLoading && !runtimeLoaded}
		<p class="placeholder-text">Loading runtime config...</p>
	{:else}
		<div class="version-row">
			<span class="config-label">Project #{projectId}</span>
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
			<Button onclick={onValidate} disabled={validating}>
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
				onclick={onReloadFromFile}
				disabled={reloading || runtimeLoading}
				variant="secondary"
			>
				{#if reloading}Reloading...{:else}Reload From File{/if}
			</Button>
			<Button onclick={onSave} disabled={runtimeSaving || runtimeLoading}>
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
					variant={runtimeResult.hot_reload_applied ? 'success' : 'warning'}
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

<style>
	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 2rem;
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
