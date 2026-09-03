<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { summaryApi, type SummaryResponse, type FileSummaryItem } from '$lib/api/summary';

	let activeTab = $state<'single' | 'batch' | 'directory'>('single');
	let filePath = $state('');
	let language = $state('');
	let result = $state<SummaryResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	// Batch mode
	let filePathsText = $state('');
	let filePaths = $derived(filePathsText.split('\n').map(s => s.trim()).filter(s => s));

	// Directory mode
	let directoryPath = $state('');
	let directoryExtensions = $state('rs,py,ts,js,go');
	let directoryExcludeDirs = $state('node_modules,target,.git');
	let respectGitignore = $state(true);

	async function handleGenerate() {
		loading = true;
		error = null;
		result = null;

		try {
			const request: any = {};

			if (activeTab === 'single') {
				request.file_paths = [filePath];
			} else if (activeTab === 'batch') {
				if (filePaths.length === 0) {
					error = 'Please enter at least one file path';
					loading = false;
					return;
				}
				request.file_paths = filePaths;
			} else {
				if (!directoryPath) {
					error = 'Please enter a directory path';
					loading = false;
					return;
				}
				request.directory_paths = [directoryPath];
				request.extensions = directoryExtensions.split(',').map(s => s.trim()).filter(s => s);
				request.exclude_dirs = directoryExcludeDirs.split(',').map(s => s.trim()).filter(s => s);
				request.respect_gitignore = respectGitignore;
			}

			result = await summaryApi.generate(request);
		} catch (e: any) {
			error = e.message || 'Failed to generate summary';
		} finally {
			loading = false;
		}
	}

	function formatMs(ms: number): string {
		if (ms < 1000) return `${ms}ms`;
		return `${(ms / 1000).toFixed(2)}s`;
	}
</script>

<svelte:head>
	<title>Summary Generator - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Summary Generator</h1>
		<p class="page-description">Generate natural language summaries of code files</p>

		{#if error}
			<div class="error-banner">
				<span>{error}</span>
				<button class="dismiss-btn" onclick={() => error = null}>×</button>
			</div>
		{/if}

		<!-- Input Card -->
		<Card title="Summary Input" subtitle="Select files or directories to summarize">
			<!-- Tab Navigation -->
			<div class="input-tabs">
				<button
					class="input-tab"
					class:active={activeTab === 'single'}
					onclick={() => activeTab = 'single'}
				>
					Single File
				</button>
				<button
					class="input-tab"
					class:active={activeTab === 'batch'}
					onclick={() => activeTab = 'batch'}
				>
					Batch Files
				</button>
				<button
					class="input-tab"
					class:active={activeTab === 'directory'}
					onclick={() => activeTab = 'directory'}
				>
					Directory Scan
				</button>
			</div>

			<!-- Single File Mode -->
			{#if activeTab === 'single'}
				<div class="input-group">
					<label class="field-label" for="file-path">File Path</label>
					<Input
						id="file-path"
						type="text"
						bind:value={filePath}
						placeholder="/path/to/file.rs"
					/>
				</div>
			{/if}

			<!-- Batch Mode -->
			{#if activeTab === 'batch'}
				<div class="input-group">
					<label class="field-label" for="file-paths">File Paths (one per line)</label>
					<textarea
						id="file-paths"
						class="textarea-input"
						bind:value={filePathsText}
						placeholder={"/path/to/file1.rs\n/path/to/file2.ts"}
						rows="6"
					></textarea>
					<span class="field-hint">{filePaths.length} file(s) entered</span>
				</div>
			{/if}

			<!-- Directory Mode -->
			{#if activeTab === 'directory'}
				<div class="input-group">
					<label class="field-label" for="dir-path">Directory Path</label>
					<Input
						id="dir-path"
						type="text"
						bind:value={directoryPath}
						placeholder="/path/to/project"
					/>
				</div>
				<div class="input-row">
					<div class="input-group">
						<label class="field-label" for="dir-ext">Extensions (comma-separated)</label>
						<Input
							id="dir-ext"
							type="text"
							bind:value={directoryExtensions}
							placeholder="rs,py,ts,js,go"
						/>
					</div>
					<div class="input-group">
						<label class="field-label" for="dir-exclude">Exclude Dirs (comma-separated)</label>
						<Input
							id="dir-exclude"
							type="text"
							bind:value={directoryExcludeDirs}
							placeholder="node_modules,target,.git"
						/>
					</div>
				</div>
				<div class="input-group checkbox-group">
					<label class="checkbox-label">
						<input type="checkbox" bind:checked={respectGitignore} />
						Respect .gitignore
					</label>
				</div>
			{/if}

			<div class="generate-actions">
				<Button onclick={handleGenerate} disabled={loading}>
					{#if loading}Generating...{:else}Generate Summary{/if}
				</Button>
			</div>
		</Card>

		<!-- Results -->
		{#if result}
			<Card title="Summary Results" subtitle={`${result.success_count} of ${result.total_files} files succeeded`}>
				<div class="result-summary">
					<div class="result-stat">
						<span class="stat-label">Success</span>
						<span class="stat-value">{result.success_count}</span>
					</div>
					<div class="result-stat">
						<span class="stat-label">Failed</span>
						<span class="stat-value">{result.failed_count}</span>
					</div>
					<div class="result-stat">
						<span class="stat-label">Time</span>
						<span class="stat-value">{formatMs(result.elapsed_ms)}</span>
					</div>
				</div>

				{#if result.warnings.length > 0}
					<div class="warnings-section">
						<h3 class="section-title">Warnings</h3>
						<ul class="warnings-list">
							{#each result.warnings as warn}
								<li class="warning-item">{warn}</li>
							{/each}
						</ul>
					</div>
				{/if}

				{#if result.summaries.length > 0}
					<div class="summary-list">
						{#each result.summaries as item}
							<details class="summary-item">
								<summary class="summary-header">
									<span class="summary-file">{item.file_path}</span>
									<Badge label={item.language} variant="info" />
									{#if !item.success}
										<Badge label="Failed" variant="danger" />
									{/if}
								</summary>
								<div class="summary-body">
									{#if !item.success && item.error}
										<div class="error-message">
											<strong>Error:</strong> {item.error}
										</div>
									{:else}
										<p class="summary-text">{item.summary}</p>
										<div class="summary-meta">
											<span>{item.entity_count} entities</span>
											<span>{item.line_count} lines</span>
											{#if item.tags.length > 0}
												<span>Tags: {item.tags.join(', ')}</span>
											{/if}
											<span>Importance: {item.importance_level}</span>
										</div>
										{#if item.main_entities.length > 0}
											<div class="summary-entities">
												<h4>Entities</h4>
												<ul>
													{#each item.main_entities as entity}
														<li>{entity}</li>
													{/each}
												</ul>
											</div>
										{/if}
										{#if item.imports.length > 0}
											<div class="summary-entities">
												<h4>Imports</h4>
												<ul>
													{#each item.imports.slice(0, 20) as imp}
														<li>{imp}</li>
													{/each}
													{#if item.imports.length > 20}
														<li class="more">... and {item.imports.length - 20} more</li>
													{/if}
												</ul>
											</div>
										{/if}
									{/if}
								</div>
							</details>
						{/each}
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

	.input-tabs {
		display: flex;
		gap: 0;
		border-bottom: 1px solid var(--gray-200);
		margin-bottom: 1.5rem;
	}

	.input-tab {
		padding: 0.75rem 1.5rem;
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		cursor: pointer;
		transition: all 0.2s ease;
		color: var(--gray-600);
	}

	.input-tab:hover {
		color: var(--black);
	}

	.input-tab.active {
		color: var(--black);
		border-bottom-color: var(--accent);
		font-weight: bold;
	}

	.input-group {
		margin-bottom: 1rem;
	}

	.input-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
	}

	.field-label {
		display: block;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.field-hint {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--gray-400);
		margin-top: 0.25rem;
		display: block;
	}

	.textarea-input {
		width: 100%;
		padding: 0.75rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		resize: vertical;
		box-sizing: border-box;
	}

	.textarea-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.checkbox-group {
		margin-top: 0.5rem;
	}

	.checkbox-label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.generate-actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 1.5rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.result-summary {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1rem;
		margin-bottom: 1.5rem;
	}

	.result-stat {
		padding: 0.75rem;
		border: 1px solid var(--gray-200);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.25rem;
	}

	.stat-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.stat-value {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.5rem;
		font-weight: 700;
	}

	.section-title {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		font-weight: 700;
		margin-bottom: 0.75rem;
		letter-spacing: -0.03em;
	}

	.warnings-section {
		margin-bottom: 1.5rem;
	}

	.warnings-list {
		list-style: none;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.warning-item {
		padding: 0.5rem 0.75rem;
		background: var(--warning-bg, #fffbe6);
		border: 1px solid var(--warning, #e6a700);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--warning-text, #8a6d00);
	}

	.summary-list {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.summary-item {
		border: 1px solid var(--gray-200);
		overflow: hidden;
	}

	.summary-header {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.75rem 1rem;
		cursor: pointer;
		background: var(--gray-50);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		user-select: none;
	}

	.summary-header:hover {
		background: var(--gray-100);
	}

	.summary-file {
		flex: 1;
		font-weight: 700;
	}

	.summary-body {
		padding: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.summary-body .error-message {
		padding: 0.75rem;
		background: var(--danger-bg, #fff2f0);
		border: 1px solid var(--danger, #ff4d4f);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		color: var(--danger-text, #cf1322);
	}

	.summary-text {
		line-height: 1.6;
		margin-bottom: 1rem;
		color: var(--gray-700);
	}

	.summary-meta {
		display: flex;
		gap: 1rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-500);
		margin-bottom: 1rem;
	}

	.summary-entities h4 {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.summary-entities ul {
		list-style: none;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}

	.summary-entities li {
		padding: 0.25rem 0.5rem;
		background: var(--gray-50);
		border: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}

	.summary-entities li.more {
		color: var(--gray-400);
		font-style: italic;
		border-style: dashed;
	}

	@media (max-width: 768px) {
		.input-row {
			grid-template-columns: 1fr;
		}

		.result-summary {
			grid-template-columns: 1fr;
		}
	}
</style>