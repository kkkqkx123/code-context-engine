<script lang="ts">
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
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
				mode: mode
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
				<div class="input-row">
					<div class="input-field">
						<label class="field-label" for="fold-language">Language</label>
						<select id="fold-language" bind:value={language} class="select-input">
							<option value="rust">Rust</option>
							<option value="python">Python</option>
							<option value="typescript">TypeScript</option>
							<option value="javascript">JavaScript</option>
							<option value="go">Go</option>
							<option value="java">Java</option>
						</select>
					</div>
					<div class="input-field flex-1">
						<label class="field-label" for="fold-filename">File Name (optional)</label>
						<input
							id="fold-filename"
							type="text"
							bind:value={fileName}
							placeholder="e.g., main.rs"
							class="text-input"
						/>
					</div>
				</div>
				<div class="input-row">
					<div class="input-field">
						<label class="field-label" for="fold-tokens">Max Tokens</label>
						<input
							id="fold-tokens"
							type="number"
							bind:value={maxTokens}
							min={1}
							max={8000}
							class="text-input"
						/>
					</div>
					<div class="input-field">
						<label class="field-label" for="fold-mode">Mode</label>
						<select id="fold-mode" bind:value={mode} class="select-input">
							<option value="detailed">Detailed</option>
							<option value="minimal">Minimal</option>
						</select>
					</div>
				</div>
				<textarea
					bind:value={text}
					placeholder="Enter code to fold into a symbol skeleton..."
					class="code-textarea"
				></textarea>
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
					<div class="empty-output">Folded skeleton will appear here...</div>
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

	.input-row {
		display: flex;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.input-field {
		display: flex;
		flex-direction: column;
	}

	.input-field.flex-1 {
		flex: 1;
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

	.select-input,
	.text-input {
		padding: 0.75rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.9rem;
		background: var(--white);
		cursor: pointer;
	}

	.select-input:focus,
	.text-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.code-textarea {
		flex: 1;
		width: 100%;
		padding: 1rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		background: var(--black);
		color: var(--white);
		resize: none;
		min-height: 300px;
	}

	.code-textarea:focus {
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
