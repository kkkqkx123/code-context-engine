<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import SplitPane from '$lib/components/ui/SplitPane.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import ToolFormRow from '$lib/components/tools/ToolFormRow.svelte';
	import ToolActions from '$lib/components/tools/ToolActions.svelte';
	import { toolsApi } from '$lib/api/tools';
	import type { Diagnostic } from '$lib/api/tools';

	interface Props {
		language?: string;
	}

	let { language = $bindable('typescript') }: Props = $props();

	let code = $state('');
	let fileName = $state('');
	let result: {
		language: string;
		is_valid: boolean;
		diagnostics: Diagnostic[];
	} | null = $state(null);
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
		} catch (err) {
			error = errorMessage(err);
		} finally {
			loading = false;
		}
	}

	function getSeverityVariant(
		kind: string,
	): 'danger' | 'warning' | 'info' | 'success' | 'default' {
		const lowerKind = kind.toLowerCase();
		if (
			lowerKind.includes('error') ||
			lowerKind.includes('missing') ||
			lowerKind.includes('unclosed') ||
			lowerKind.includes('illegal')
		) {
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

<ToolShell {error}>
	<SplitPane leftWidth={50}>
		{#snippet left()}
			<div class="tool-input">
				<ToolFormRow>
					<ToolField label="Language">
						<select
							id="diagnose-language"
							class="text-input"
							bind:value={language}
						>
							<option value="typescript">TypeScript</option>
							<option value="javascript">JavaScript</option>
							<option value="rust">Rust</option>
							<option value="python">Python</option>
							<option value="go">Go</option>
							<option value="java">Java</option>
						</select>
					</ToolField>
					<ToolField label="File Name (optional)" wide>
						<input
							id="diagnose-filename"
							type="text"
							bind:value={fileName}
							placeholder="e.g., main.ts"
							class="text-input"
						/>
					</ToolField>
				</ToolFormRow>
				<textarea
					bind:value={code}
					placeholder="Enter code to diagnose..."
					class="code-textarea"></textarea>
				<ToolActions>
					<Button onclick={handleDiagnose} disabled={!code.trim() || loading}>
						{#if loading}Diagnosing...{:else}Diagnose{/if}
					</Button>
				</ToolActions>
			</div>
		{/snippet}

		{#snippet right()}
			<div class="tool-output">
				{#if result}
					{#if result.diagnostics.length === 0}
						<div class="no-issues">No issues found <Icon name="check" /></div>
					{:else}
						<div class="issues-list">
							{#each result.diagnostics as diagnostic (diagnostic.message + diagnostic.position.row + ':' + diagnostic.position.column)}
								<div
									class="issue-card"
									style="border-left-color: {getSeverityBordercolor(
										diagnostic.kind,
									)}"
								>
									<div class="issue-header">
										<Badge
											label={diagnostic.kind}
											variant={getSeverityVariant(diagnostic.kind)}
										/>
										<span class="issue-location">
											Line {diagnostic.position.row + 1}:{diagnostic.position
												.column + 1}
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
					<EmptyState fill message="Diagnostic results will appear here..." />
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
