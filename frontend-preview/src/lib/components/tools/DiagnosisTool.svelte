<script lang="ts">
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { toolsApi } from '$lib/api/tools';
	import type { Diagnostic } from '$lib/api/tools';

	interface Props {
		language?: string;
	}

	let { language = $bindable('typescript') }: Props = $props();

	let code = $state('');
	let fileName = $state('');
	let result: { language: string; is_valid: boolean; diagnostics: Diagnostic[] } | null = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleDiagnose() {
		if (!code.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			result = await toolsApi.diagnose({
				code: code,
				language: language,
				file_name: fileName || undefined,
			});
		} catch (err: any) {
			error = err.message;
		} finally {
			loading = false;
		}
	}

	function getSeverityVariant(kind: string): 'danger' | 'warning' | 'info' | 'success' | 'default' {
		const lowerKind = kind.toLowerCase();
		if (lowerKind.includes('error') || lowerKind.includes('missing') || lowerKind.includes('unclosed') || lowerKind.includes('illegal')) {
			return 'danger';
		}
		if (lowerKind.includes('incomplete') || lowerKind.includes('indentation')) {
			return 'warning';
		}
		return 'info';
	}

	function getSeverityBordercolor(kind: string): string {
		const variant = getSeverityVariant(kind);
		switch (variant) {
			case 'danger':
				return 'var(--danger)';
			case 'warning':
				return 'var(--warning)';
			case 'info':
				return 'var(--info)';
			default:
				return 'var(--black)';
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
						<label class="field-label" for="diagnose-language">Language</label>
						<select id="diagnose-language" bind:value={language} class="select-input">
							<option value="typescript">TypeScript</option>
							<option value="javascript">JavaScript</option>
							<option value="rust">Rust</option>
							<option value="python">Python</option>
							<option value="go">Go</option>
							<option value="java">Java</option>
						</select>
					</div>
					<div class="input-field flex-1">
						<label class="field-label" for="diagnose-filename">File Name (optional)</label>
						<input
							id="diagnose-filename"
							type="text"
							bind:value={fileName}
							placeholder="e.g., main.ts"
							class="text-input"
						/>
					</div>
				</div>
				<textarea
					bind:value={code}
					placeholder="Enter code to diagnose..."
					class="code-textarea"
				></textarea>
				<div class="tool-actions">
					<Button
						onclick={handleDiagnose}
						disabled={!code.trim() || loading}
					>
						{#if loading}Diagnosing...{:else}Diagnose{/if}
					</Button>
				</div>
			</div>
		{/snippet}

		{#snippet right()}
			<div class="tool-output">
				{#if result}
					{#if result.diagnostics.length === 0}
						<div class="no-issues">No issues found ✓</div>
					{:else}
						<div class="issues-list">
							{#each result.diagnostics as diagnostic}
								<div
									class="issue-card"
									style="border-left-color: {getSeverityBordercolor(diagnostic.kind)}"
								>
									<div class="issue-header">
										<Badge
											label={diagnostic.kind}
											variant={getSeverityVariant(diagnostic.kind)}
										/>
										<span class="issue-location">
											Line {diagnostic.position.row + 1}:{diagnostic.position.column + 1}
										</span>
									</div>
									<p class="issue-message">{diagnostic.message}</p>
									<div class="issue-precision">
										Precision: {diagnostic.precision}
									</div>
								</div>
							{/each}
						</div>
					{/if}
				{:else}
					<div class="empty-output">Diagnostic results will appear here...</div>
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

	.no-issues {
		text-align: center;
		padding: 3rem;
		color: var(--gray-600);
		font-size: 1.1rem;
	}

	.issues-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.issue-card {
		padding: 1rem;
		border: 1px solid var(--gray-200);
		border-left: 3px solid var(--black);
	}

	.issue-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.75rem;
	}

	.issue-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-600);
	}

	.issue-message {
		color: var(--gray-800);
		line-height: 1.6;
		margin-bottom: 0.75rem;
	}

	.issue-precision {
		font-size: 0.8rem;
		color: var(--gray-500);
		font-style: italic;
	}
</style>
