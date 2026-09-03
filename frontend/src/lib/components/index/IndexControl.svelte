<script lang="ts">
	import { indexState, indexActions, selectedProject } from '$lib/stores/index';
	import Card from '../ui/Card.svelte';
	import Button from '../ui/Button.svelte';
	import Input from '../ui/Input.svelte';
	import ProgressBar from '../ui/ProgressBar.svelte';
	import Badge from '../ui/Badge.svelte';

	// Form state
	let indexPath = $state('');
	let extensions = $state('');
	let excludePatterns = $state('');
	let forceReindex = $state(false);
	let respectGitignore = $state(true);
	let isIncremental = $state(false);

	function resetForm() {
		indexPath = '';
		extensions = '';
		excludePatterns = '';
		forceReindex = false;
		respectGitignore = true;
		isIncremental = false;
	}

	async function handleIndex() {
		if (!indexPath) {
			alert('Directory path is required');
			return;
		}

		const data: any = {
			path: indexPath,
			force: forceReindex,
			gitignore: respectGitignore,
		};

		if (extensions) {
			data.extensions = extensions.split(',').map(e => e.trim()).filter(Boolean);
		}

		if (excludePatterns) {
			data.exclude = excludePatterns.split(',').map(e => e.trim()).filter(Boolean);
		}

		try {
			if (isIncremental) {
				await indexActions.startIncrementalIndex(data);
			} else {
				await indexActions.startIndex(data);
			}
			resetForm();
		} catch (error: any) {
			alert(`Failed to start indexing: ${error.message}`);
		}
	}

	function handleCancel() {
		indexActions.stopIndex();
	}

	function useSelectedProject() {
		if ($selectedProject) {
			indexPath = $selectedProject.root_path;
			extensions = $selectedProject.extensions?.join(', ') || '';
			excludePatterns = $selectedProject.exclude_dirs?.join(', ') || '';
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

			<ProgressBar 
				progress={$indexState.progress} 
				showLabel={true}
			/>

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
				<Button variant="danger" onclick={handleCancel}>
					Cancel Indexing
				</Button>
			</div>
		</div>
	{:else}
		<div class="form-section">
			<div class="form-header">
				<h3>Configure Index Operation</h3>
				{#if $selectedProject}
					<Button variant="secondary" size="sm" onclick={useSelectedProject}>
						Use Selected Project
					</Button>
				{/if}
			</div>

			<form onsubmit={(e) => { e.preventDefault(); handleIndex(); }}>
				<Input
					label="Directory Path"
					type="text"
					bind:value={indexPath}
					required={true}
					placeholder="/path/to/directory"
				/>

				<Input
					label="File Extensions (comma-separated)"
					type="text"
					bind:value={extensions}
					placeholder="rs, ts, js, py, go"
				/>

				<Input
					label="Exclude Patterns (comma-separated)"
					type="text"
					bind:value={excludePatterns}
					placeholder="node_modules, target, .git, dist"
				/>

				<div class="toggle-group">
					<label class="toggle-item">
						<input type="checkbox" bind:checked={forceReindex} />
						<span class="toggle-label">Force Re-index</span>
						<span class="toggle-description">Ignore cache and re-parse all files</span>
					</label>

					<label class="toggle-item">
						<input type="checkbox" bind:checked={respectGitignore} />
						<span class="toggle-label">Respect .gitignore</span>
						<span class="toggle-description">Skip files listed in .gitignore</span>
					</label>

					<label class="toggle-item">
						<input type="checkbox" bind:checked={isIncremental} />
						<span class="toggle-label">Incremental Mode</span>
						<span class="toggle-description">Only process changed files</span>
					</label>
				</div>

				<div class="form-actions">
					<Button type="submit" variant="primary">
						Start Indexing
					</Button>
					<Button type="button" variant="secondary" onclick={resetForm}>
						Reset
					</Button>
				</div>
			</form>
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

	.toggle-item input[type="checkbox"] {
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
		margin-top: 1rem;
	}
</style>
