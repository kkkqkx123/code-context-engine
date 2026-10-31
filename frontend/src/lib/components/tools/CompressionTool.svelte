<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import { toolsApi } from '$lib/api/tools';
	import type { CompressResult } from '$lib/api/tools';

	let filePath = $state('');
	let includeEntities = $state(false);
	let includeGroups = $state(false);
	let result: CompressResult | null = $state(null);
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
				include_entities: includeEntities || undefined,
				include_groups: includeGroups || undefined,
			});
		} catch (err) {
			error = errorMessage(err);
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<SplitPane leftWidth={50}>
		{#snippet left()}
			<div class="tool-input">
				<ToolField label="File Path" wide>
					<input
						id="file-path"
						type="text"
						bind:value={filePath}
						placeholder="e.g., /path/to/file.ts"
						class="text-input"
					/>
				</ToolField>
				<div class="tool-actions">
					<Toggle
						checked={includeEntities}
						label="Include entities"
						onchange={(e) => (includeEntities = e.checked)}
					/>
					<Toggle
						checked={includeGroups}
						label="Include groups"
						onchange={(e) => (includeGroups = e.checked)}
					/>
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
					<EmptyState fill message="Results will appear here..." />
				{/if}
			</div>
		{/snippet}
	</SplitPane>
</ToolShell>

<style>
	.tool-input,
	.tool-output {
		height: 100%;
		display: flex;
		flex-direction: column;
		padding: 1rem;
	}

	.tool-actions {
		margin-top: 1rem;
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 1rem;
		flex-wrap: wrap;
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
