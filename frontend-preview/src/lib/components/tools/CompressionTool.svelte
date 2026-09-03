<script lang="ts">
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import CodeBlock from '$lib/components/ui/CodeBlock.svelte';
	import { toolsApi } from '$lib/api/tools';

	let filePath = $state('');
	let language = $state('typescript');
	let result: any = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleCompress() {
		if (!filePath.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			result = await toolsApi.compress({
				file_path: filePath,
			});
		} catch (err: any) {
			error = err.message;
		} finally {
			loading = false;
		}
	}
</script>

<div class="tool-content">
	{#if error}
		<div class="error-message">{error}</div>
	{/if}

	<SplitPane leftWidth={50}>
		{#snippet left()}
			<div class="tool-input">
				<div class="input-header">
					<label class="field-label" for="file-path">File Path</label>
					<input
						id="file-path"
						type="text"
						bind:value={filePath}
						placeholder="e.g., /path/to/file.ts"
						class="file-path-input"
					/>
				</div>
				<div class="input-header">
					<label class="field-label" for="compress-language">Language (optional)</label>
					<select id="compress-language" bind:value={language} class="select-input">
						<option value="typescript">TypeScript</option>
						<option value="javascript">JavaScript</option>
						<option value="rust">Rust</option>
						<option value="python">Python</option>
						<option value="go">Go</option>
						<option value="java">Java</option>
					</select>
				</div>
				<div class="tool-actions">
					<Button
						onclick={handleCompress}
						disabled={!filePath.trim() || loading}
					>
						{#if loading}Compressing...{:else}Compress{/if}
					</Button>
				</div>
			</div>
		{/snippet}

		{#snippet right()}
			<div class="tool-output">
				{#if result}
					<div class="compression-stats">
						<div class="stat-item">
							<span class="stat-label">Original Tokens:</span>
							<span class="stat-value">{result.original_tokens}</span>
						</div>
						<div class="stat-item">
							<span class="stat-label">Compressed Tokens:</span>
							<span class="stat-value">{result.compressed_tokens}</span>
						</div>
						<div class="stat-item highlight">
							<span class="stat-label">Reduction:</span>
							<span class="stat-value">{result.reduction_percentage.toFixed(1)}%</span>
						</div>
					</div>
					<CodeBlock code={result.compressed_code} language={language} />
				{:else}
					<div class="empty-output">Results will appear here...</div>
				{/if}
			</div>
		{/snippet}
	</SplitPane>
</div>

<style>
	.tool-content {
		width: 100%;
	}

	.error-message {
		background: var(--danger);
		color: var(--white);
		padding: 1rem;
		margin-bottom: 1.5rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.tool-input,
	.tool-output {
		height: 100%;
		display: flex;
		flex-direction: column;
		padding: 1rem;
	}

	.input-header {
		margin-bottom: 1rem;
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

	.select-input {
		width: 100%;
		padding: 0.75rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.9rem;
		background: var(--white);
		cursor: pointer;
	}

	.select-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.file-path-input {
		width: 100%;
		padding: 0.75rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.9rem;
		background: var(--black);
		color: var(--white);
	}

	.file-path-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.tool-actions {
		margin-top: 1rem;
		display: flex;
		justify-content: flex-end;
	}

	.empty-output {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--gray-500);
		font-style: italic;
	}

	.compression-stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1rem;
		margin-bottom: 1.5rem;
	}

	.stat-item {
		padding: 1rem;
		border: 1px solid var(--gray-200);
		text-align: center;
	}

	.stat-item.highlight {
		border-color: var(--accent);
		background: var(--accent-bg);
	}

	.stat-label {
		display: block;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.stat-value {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.5rem;
		font-weight: 700;
		color: var(--black);
	}

	@media (max-width: 1024px) {
		.compression-stats {
			grid-template-columns: 1fr;
		}
	}
</style>
