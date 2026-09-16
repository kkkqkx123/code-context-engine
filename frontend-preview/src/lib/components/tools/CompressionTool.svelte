<script lang="ts">
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { toolsApi } from '$lib/api/tools';
	import type { CompressApiResponse } from '$lib/api/tools';

	let filePath = $state('');
	let result: CompressApiResponse | null = $state(null);
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
					<div class="compression-info">
						<div class="info-row">
							<span class="info-label">Language:</span>
							<span class="info-value">{result.language}</span>
						</div>
						<div class="info-row">
							<span class="info-label">File Hash:</span>
							<span class="info-value hash">{result.file_hash}</span>
						</div>
						<div class="info-row">
							<span class="info-label">From Cache:</span>
							<span class="info-value">{result.from_cache ? 'Yes' : 'No'}</span>
						</div>
					</div>
					<div class="semantic-text">
						<h3>Semantic Text</h3>
						<div class="text-content">{result.semantic_text}</div>
					</div>
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

	.compression-info {
		padding: 1rem;
		border: 1px solid var(--gray-200);
		margin-bottom: 1.5rem;
	}

	.info-row {
		display: flex;
		justify-content: space-between;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--gray-100);
	}

	.info-row:last-child {
		border-bottom: none;
	}

	.info-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.info-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		color: var(--black);
	}

	.info-value.hash {
		font-size: 0.7rem;
		word-break: break-all;
	}

	.semantic-text {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.semantic-text h3 {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 1rem;
	}

	.text-content {
		flex: 1;
		padding: 1rem;
		background: var(--gray-50);
		border: 1px solid var(--gray-200);
		font-family: 'Space Grotesk', sans-serif;
		font-size: 0.95rem;
		line-height: 1.6;
		color: var(--gray-800);
		white-space: pre-wrap;
		overflow-y: auto;
	}
</style>
