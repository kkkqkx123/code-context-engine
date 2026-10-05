<script lang="ts">
	import { indexState, indexActions } from '$lib/stores/index';
	import { currentProject } from '$lib/stores/project';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Card from '../ui/Card.svelte';
	import Button from '../ui/Button.svelte';
	import ProgressBar from '../ui/ProgressBar.svelte';
	import Badge from '../ui/Badge.svelte';

	// Form state
	let incremental = $state(false);
	let forceReindex = $state(false);

	function resetForm() {
		incremental = false;
		forceReindex = false;
	}

	function goToProjects() {
		goto(resolve('/projects'));
	}

	async function handleIndex() {
		const project = $currentProject;
		if (!project) return;

		if (incremental) {
			await indexActions.startIncrementalIndex({
				project_id: Number(project.id),
				force_reindex: forceReindex,
			});
		} else {
			await indexActions.startProjectIndex(project.id);
		}
	}
</script>

<Card title="Index Control" subtitle="Start and monitor indexing operations">
	{#if $indexState.isIndexing}
		<div class="progress-section">
			<div class="progress-header">
				<h3>Indexing in Progress</h3>
				<Badge variant="active">{$indexState.phase || 'Processing'}</Badge>
			</div>

			<ProgressBar progress={$indexState.progress} showLabel={true} />

			{#if $indexState.currentFile}
				<div class="current-file">
					<span class="file-label">Current File:</span>
					<span class="file-path">{$indexState.currentFile}</span>
				</div>
			{/if}

			{#if $indexState.errorCount > 0}
				<div class="error-info">
					<span class="error-count">Errors: {$indexState.errorCount}</span>
					{#if $indexState.lastError}
						<p class="error-message">{$indexState.lastError}</p>
					{/if}
				</div>
			{/if}

			<div class="progress-actions">
				<Button variant="danger" onclick={indexActions.stopIndex}>
					Cancel Indexing
				</Button>
			</div>
		</div>
	{:else}
		<div class="form-section">
			<div class="form-header">
				<h3>Configure Index Operation</h3>
			</div>

			{#if $currentProject}
				<div class="target-project">
					<span class="target-label">Target</span>
					<span class="target-name">{$currentProject.name}</span>
					<span class="target-path" title={$currentProject.root_path}
						>{$currentProject.root_path}</span
					>
				</div>

				<form
					onsubmit={(e) => {
						e.preventDefault();
						handleIndex();
					}}
				>
					<div class="toggle-group">
						<label class="toggle-item">
							<input type="checkbox" bind:checked={incremental} />
							<span class="toggle-label">Incremental Mode</span>
							<span class="toggle-description">Only process changed files</span>
						</label>

						{#if incremental}
							<label class="toggle-item">
								<input type="checkbox" bind:checked={forceReindex} />
								<span class="toggle-label">Force Re-index</span>
								<span class="toggle-description"
									>Ignore cache and re-parse all files</span
								>
							</label>
						{/if}
					</div>

					<p class="config-hint">
						Path, extensions, excluded directories and ignore rules come from
						the project configuration.
					</p>

					<div class="form-actions">
						<Button type="submit" variant="primary">Start Indexing</Button>
						<Button type="button" variant="secondary" onclick={resetForm}>
							Reset
						</Button>
					</div>
				</form>
			{:else}
				<div class="empty-state">
					<p>No project selected. Create a project to start indexing.</p>
					<Button variant="primary" onclick={goToProjects}>
						Go to Projects
					</Button>
				</div>
			{/if}
		</div>
	{/if}
</Card>

<style>
	.progress-section {
		padding: 1.5rem;
		border: 1px solid var(--black);
		background: var(--gray-100);
	}

	.progress-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1.5rem;
	}

	.progress-header h3 {
		font-size: 1.25rem;
		margin: 0;
	}

	.current-file {
		margin-top: 1.5rem;
		padding: 1rem;
		background: var(--white);
		border: 1px solid var(--gray-200);
	}

	.file-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		display: block;
		margin-bottom: 0.5rem;
	}

	.file-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		word-break: break-all;
	}

	.error-info {
		margin-top: 1rem;
		padding: 1rem;
		background: var(--white);
		border-left: 3px solid var(--danger);
	}

	.error-count {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 700;
		color: var(--danger);
	}

	.error-message {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
		margin-top: 0.5rem;
	}

	.progress-actions {
		margin-top: 1.5rem;
	}

	.form-section {
		padding: 1rem 0;
	}

	.form-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1.5rem;
	}

	.form-header h3 {
		font-size: 1.25rem;
		margin: 0;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.target-project {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		padding: 1rem;
		border: 1px solid var(--gray-200);
		background: var(--gray-100);
		min-width: 0;
	}

	.target-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.target-name {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 700;
		font-size: 1rem;
	}

	.target-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.config-hint {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: 0;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1rem;
		padding: 1.5rem;
		border: 1px dashed var(--gray-300);
		background: var(--gray-100);
	}

	.empty-state p {
		margin: 0;
		color: var(--gray-600);
	}

	.toggle-group {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding: 1rem;
		border: 1px solid var(--gray-200);
		background: var(--gray-100);
	}

	.toggle-item {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		cursor: pointer;
	}

	.toggle-item input[type='checkbox'] {
		width: auto;
		margin-right: 0.5rem;
	}

	.toggle-label {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 500;
		font-size: 0.95rem;
	}

	.toggle-description {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
		margin-left: 1.5rem;
	}

	.form-actions {
		display: flex;
		gap: 1rem;
	}
</style>
