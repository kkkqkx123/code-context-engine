<script lang="ts">
	/**
	 * Project basics form card for the config page: identity and file
	 * filters stored in the project registry. All form state is bound from
	 * the page so unsaved edits survive tab switches; the page owns load
	 * and save.
	 */
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';

	interface Props {
		rootPath?: string;
		basicName?: string;
		basicExtensions?: string;
		basicExcludeDirs?: string;
		basicIgnorePatterns?: string;
		basicRespectGitignore?: boolean;
		basicLoaded?: boolean;
		basicLoading?: boolean;
		basicSaving?: boolean;
		basicError?: string | null;
		basicResult?: string | null;
		onSave: () => void;
	}

	let {
		rootPath = '',
		basicName = $bindable(''),
		basicExtensions = $bindable(''),
		basicExcludeDirs = $bindable(''),
		basicIgnorePatterns = $bindable(''),
		basicRespectGitignore = $bindable(true),
		basicLoaded = false,
		basicLoading = false,
		basicSaving = false,
		basicError = null,
		basicResult = null,
		onSave,
	}: Props = $props();
</script>

<Card
	title="Project Basics"
	subtitle="Identity and file filters stored in the project registry"
>
	{#if basicLoading && !basicLoaded}
		<p class="empty-hint">Loading project...</p>
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
				<Input
					bind:value={basicExcludeDirs}
					placeholder="target, node_modules"
				/>
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
			<Button onclick={onSave} disabled={basicSaving || basicLoading}>
				{#if basicSaving}Saving...{:else}Save Basics{/if}
			</Button>
		</div>

		{#if basicError}
			<InlineError message={basicError} />
		{/if}
		{#if basicResult}
			<div class="reload-result">
				<Badge label="Saved" variant="active" />
				<span class="reload-message">{basicResult}</span>
			</div>
		{/if}
	{/if}
</Card>

<style>
	.toggle-row {
		margin: 0.5rem 0 1rem;
	}
</style>
