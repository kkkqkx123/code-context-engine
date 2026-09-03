<script lang="ts">
	import { indexApi, type ParseResult } from '$lib/api/index';
	import Card from '../ui/Card.svelte';
	import Button from '../ui/Button.svelte';
	import Input from '../ui/Input.svelte';
	import CodeBlock from '../ui/CodeBlock.svelte';
	import Badge from '../ui/Badge.svelte';

	let filePath = $state('');
	let language = $state('');
	let isLoading = $state(false);
	let parseResult = $state<ParseResult | null>(null);
	let error = $state<string | null>(null);

	async function handleParse() {
		if (!filePath) {
			alert('File path is required');
			return;
		}

		isLoading = true;
		error = null;
		parseResult = null;

		try {
			const result = await indexApi.parseFile(filePath, language || undefined);
			parseResult = result as ParseResult;
		} catch (err: any) {
			error = err.message || 'Failed to parse file';
		} finally {
			isLoading = false;
		}
	}

	function resetForm() {
		filePath = '';
		language = '';
		parseResult = null;
		error = null;
	}
</script>

<Card title="File Parser Preview" subtitle="Parse single file and view extracted entities">
	<div class="parser-section">
		<form onsubmit={(e) => { e.preventDefault(); handleParse(); }}>
			<Input
				label="File Path"
				type="text"
				bind:value={filePath}
				required={true}
				placeholder="/path/to/file.rs"
			/>

			<Input
				label="Language (optional)"
				type="text"
				bind:value={language}
				placeholder="rust, typescript, python"
			/>

			<div class="form-actions">
				<Button type="submit" variant="primary" disabled={isLoading}>
					{#if isLoading}
						Parsing...
					{:else}
						Parse File
					{/if}
				</Button>
				<Button type="button" variant="secondary" onclick={resetForm}>
					Clear
				</Button>
			</div>
		</form>
	</div>

	{#if error}
		<div class="error-display">
			<Badge variant="active">Error</Badge>
			<p>{error}</p>
		</div>
	{/if}

	{#if parseResult}
		<div class="results-section">
			<div class="result-header">
				<h3>Parse Results</h3>
				<div class="result-meta">
					<Badge variant="active">{parseResult.language}</Badge>
					<span class="entity-count">{parseResult.entities.length} entities found</span>
				</div>
			</div>

			<div class="file-info">
				<span class="info-label">File:</span>
				<span class="info-value">{parseResult.file_path}</span>
			</div>

			<div class="entities-list">
				<h4>Extracted Entities</h4>
				{#if parseResult.entities.length === 0}
					<p class="no-entities">No entities found in this file</p>
				{:else}
					{#each parseResult.entities as entity, index}
						<div class="entity-item">
							<div class="entity-header">
								<span class="entity-type">{entity.type || 'Unknown'}</span>
								<span class="entity-name">{entity.name || 'Unnamed'}</span>
							</div>
							{#if entity.description || entity.nl_description}
								<p class="entity-description">
									{entity.description || entity.nl_description}
								</p>
							{/if}
							{#if entity.location}
								<div class="entity-location">
									<span class="location-label">Location:</span>
									<span class="location-value">
										Line {entity.location.start_line || '?'} - {entity.location.end_line || '?'}
									</span>
								</div>
							{/if}
						</div>
					{/each}
				{/if}
			</div>

			{#if parseResult.entities.length > 0}
				<div class="raw-data">
					<h4>Raw JSON Output</h4>
					<CodeBlock language="json" code={JSON.stringify(parseResult.entities, null, 2)} />
				</div>
			{/if}
		</div>
	{/if}
</Card>

<style>
	.parser-section {
		margin-bottom: 2rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.form-actions {
		display: flex;
		gap: 1rem;
	}

	.error-display {
		padding: 1.5rem;
		border: 1px solid var(--danger);
		border-left: 4px solid var(--danger);
		background: var(--danger-bg);
		margin-bottom: 2rem;
	}

	.error-display p {
		margin-top: 0.75rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		color: var(--danger);
	}

	.results-section {
		border-top: 1px solid var(--gray-200);
		padding-top: 2rem;
	}

	.result-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1.5rem;
	}

	.result-header h3 {
		font-size: 1.25rem;
		margin: 0;
	}

	.result-meta {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.entity-count {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.file-info {
		padding: 1rem;
		background: var(--gray-100);
		border: 1px solid var(--gray-200);
		margin-bottom: 2rem;
	}

	.info-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-right: 0.5rem;
	}

	.info-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.entities-list {
		margin-bottom: 2rem;
	}

	.entities-list h4 {
		font-size: 1rem;
		margin-bottom: 1rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.no-entities {
		color: var(--gray-400);
		font-style: italic;
		padding: 2rem 0;
		text-align: center;
	}

	.entity-item {
		padding: 1.25rem;
		border: 1px solid var(--black);
		margin-bottom: 1rem;
		transition: all 0.2s;
	}

	.entity-item:hover {
		background: var(--gray-100);
	}

	.entity-header {
		display: flex;
		gap: 1rem;
		align-items: center;
		margin-bottom: 0.75rem;
	}

	.entity-type {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		padding: 0.25rem 0.5rem;
		background: var(--black);
		color: var(--white);
	}

	.entity-name {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 700;
		font-size: 1.1rem;
	}

	.entity-description {
		font-size: 0.95rem;
		color: var(--gray-600);
		line-height: 1.6;
		margin-bottom: 0.75rem;
	}

	.entity-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.location-label {
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-right: 0.5rem;
	}

	.raw-data {
		margin-top: 2rem;
	}

	.raw-data h4 {
		font-size: 1rem;
		margin-bottom: 1rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
</style>
