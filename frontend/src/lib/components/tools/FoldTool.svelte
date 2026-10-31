<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import { toolsApi } from '$lib/api/tools';
	import type { FoldResponse } from '$lib/api/tools';

	interface Props {
		language?: string;
	}

	let { language = $bindable('rust') }: Props = $props();

	let text = $state('');
	let fileName = $state('');
	let maxTokens = $state(2000);
	let mode = $state('detailed');
	let result: FoldResponse | null = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleFold() {
		if (!text.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			result = await toolsApi.fold({
				text: text,
				language: language,
				file_name: fileName || undefined,
				max_tokens: maxTokens,
				mode: mode,
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
				<div class="tool-form-row">
					<ToolField label="Language">
						<select id="fold-language" class="text-input" bind:value={language}>
							<option value="rust">Rust</option>
							<option value="python">Python</option>
							<option value="typescript">TypeScript</option>
							<option value="javascript">JavaScript</option>
							<option value="go">Go</option>
							<option value="java">Java</option>
						</select>
					</ToolField>
					<ToolField label="File Name (optional)" wide>
						<input
							id="fold-filename"
							type="text"
							bind:value={fileName}
							placeholder="e.g., main.rs"
							class="text-input"
						/>
					</ToolField>
				</div>
				<div class="tool-form-row">
					<ToolField label="Max Tokens">
						<input
							id="fold-tokens"
							type="number"
							bind:value={maxTokens}
							min={1}
							max={8000}
							class="text-input"
						/>
					</ToolField>
					<ToolField label="Mode">
						<select id="fold-mode" class="text-input" bind:value={mode}>
							<option value="detailed">Detailed</option>
							<option value="minimal">Minimal</option>
						</select>
					</ToolField>
				</div>
				<textarea
					bind:value={text}
					placeholder="Enter code to fold into a symbol skeleton..."
					class="code-textarea"></textarea>
				<div class="tool-actions">
					<Button onclick={handleFold} disabled={!text.trim() || loading}>
						{#if loading}Folding...{:else}Fold{/if}
					</Button>
				</div>
			</div>
		{/snippet}

		{#snippet right()}
			<div class="tool-output">
				{#if result}
					<div class="fold-meta">
						<Badge
							label={result.structure_known ? 'structured' : 'degraded'}
							variant={result.structure_known ? 'success' : 'warning'}
						/>
						<span class="meta-item">Language: {result.language}</span>
						<span class="meta-item">
							Tokens: {result.folded_tokens} / {result.original_tokens}
						</span>
						<span class="meta-item">
							Sections: {result.kept_sections} kept, {result.dropped_sections} dropped
						</span>
					</div>
					<pre class="fold-output">{result.folded_text}</pre>
				{:else}
					<EmptyState fill message="Folded skeleton will appear here..." />
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
	}

	.fold-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.meta-item {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.fold-output {
		flex: 1;
		padding: 1rem;
		border: 1px solid var(--gray-200);
		background: var(--black);
		color: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		line-height: 1.6;
		white-space: pre-wrap;
		word-break: break-word;
		overflow-y: auto;
	}
</style>
