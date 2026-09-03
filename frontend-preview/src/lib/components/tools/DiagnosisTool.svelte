<script lang="ts">
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { toolsApi } from '$lib/api/tools';

	interface Props {
		language?: string;
	}

	let { language = $bindable('typescript') }: Props = $props();

	let filePath = $state('');
	let result: any = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleDiagnose() {
		if (!filePath.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			result = await toolsApi.diagnose({
				code: filePath,
				language: language,
				file_name: filePath,
			});
		} catch (err: any) {
			error = err.message;
		} finally {
			loading = false;
		}
	}

	function getSeverityVariant(severity: string): 'danger' | 'warning' | 'info' | 'success' | 'default' {
		switch (severity.toLowerCase()) {
			case 'error':
				return 'danger';
			case 'warning':
				return 'warning';
			case 'info':
				return 'info';
			case 'success':
				return 'success';
			default:
				return 'default';
		}
	}

	function getSeverityBordercolor(severity: string): string {
		switch (severity.toLowerCase()) {
			case 'error':
				return 'var(--danger)';
			case 'warning':
				return 'var(--warning)';
			case 'info':
				return 'var(--info)';
			case 'success':
				return 'var(--success)';
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
				<div class="input-header">
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
				<textarea
					bind:value={filePath}
					placeholder="Enter file path to diagnose..."
					class="code-textarea"
				></textarea>
				<div class="tool-actions">
					<Button
						onclick={handleDiagnose}
						disabled={!filePath.trim() || loading}
					>
						{#if loading}Diagnosing...{:else}Diagnose{/if}
					</Button>
				</div>
			</div>
		{/snippet}

		{#snippet right()}
			<div class="tool-output">
				{#if result?.result?.issues}
					{#if result.result.issues.length === 0}
						<div class="no-issues">No issues found ✓</div>
					{:else}
						<div class="issues-list">
							{#each result.result.issues as issue}
								<div
									class="issue-card"
									style="border-left-color: {getSeverityBordercolor(issue.severity)}"
								>
									<div class="issue-header">
										<Badge
											label={issue.severity.toUpperCase()}
											variant={getSeverityVariant(issue.severity)}
										/>
										{#if issue.line !== undefined}
											<span class="issue-location">Line {issue.line}{issue.column ? `:${issue.column}` : ''}</span>
										{/if}
									</div>
									<p class="issue-message">{issue.message}</p>
									{#if issue.suggestion}
										<div class="issue-suggestion">
											<strong>Suggestion:</strong> {issue.suggestion}
										</div>
									{/if}
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

	.issue-suggestion {
		padding: 0.75rem;
		background: var(--gray-100);
		border-left: 2px solid var(--gray-500);
		font-size: 0.9rem;
		color: var(--gray-600);
	}
</style>
