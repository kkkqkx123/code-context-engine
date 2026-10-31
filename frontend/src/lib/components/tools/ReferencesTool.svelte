<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import { toolsApi, type FindReferencesResult } from '$lib/api/tools';
	import { projectStore } from '$lib/stores/project.svelte';

	let filePath = $state('');
	let line = $state(1);
	let column = $state(1);
	let symbol = $state('');
	let contextLines = $state<number | null>(null);
	let includeSnippet: boolean | null = $state(null);
	let includeEntityInfo: boolean | null = $state(null);
	let result = $state<FindReferencesResult | null>(null);
	let loading = $state(false);
	let error = $state('');

	async function handleFind() {
		const path = filePath.trim();
		if (!path) return;
		loading = true;
		error = '';
		result = null;
		try {
			result = await toolsApi.findReferences({
				project_id: projectStore.currentProjectId,
				path,
				line,
				column,
				symbol: symbol.trim() || undefined,
				context_lines: contextLines ?? undefined,
				include_snippet: includeSnippet,
				include_entity_info: includeEntityInfo,
			});
		} catch (e) {
			error = errorMessage(e) ?? 'Find references failed';
		} finally {
			loading = false;
		}
	}

	function triValue(raw: string): boolean | null {
		if (raw === 'on') return true;
		if (raw === 'off') return false;
		return null;
	}
</script>

<ToolShell {error}>
	<div class="tool-form-row">
		<ToolField label="File Path" wide>
			<input
				class="text-input"
				type="text"
				bind:value={filePath}
				placeholder="/path/to/file.ts"
			/>
		</ToolField>
		<ToolField label="Line">
			<input class="text-input" type="number" min="1" bind:value={line} />
		</ToolField>
		<ToolField label="Column">
			<input class="text-input" type="number" min="1" bind:value={column} />
		</ToolField>
		<ToolField label="Symbol (optional)">
			<input
				class="text-input"
				type="text"
				bind:value={symbol}
				placeholder="authenticate"
			/>
		</ToolField>
		<ToolField label="Context lines">
			<input
				class="text-input"
				type="number"
				min="0"
				value={contextLines ?? ''}
				oninput={(e) => {
					const n = Number((e.target as HTMLInputElement).value);
					contextLines = Number.isInteger(n) && n >= 0 ? n : null;
				}}
			/>
		</ToolField>
		<ToolField label="Snippets">
			<select
				class="text-input"
				value={includeSnippet === true
					? 'on'
					: includeSnippet === false
						? 'off'
						: ''}
				onchange={(e) =>
					(includeSnippet = triValue((e.target as HTMLSelectElement).value))}
			>
				<option value="">Default</option>
				<option value="on">On</option>
				<option value="off">Off</option>
			</select>
		</ToolField>
		<ToolField label="Entity info">
			<select
				class="text-input"
				value={includeEntityInfo === true
					? 'on'
					: includeEntityInfo === false
						? 'off'
						: ''}
				onchange={(e) =>
					(includeEntityInfo = triValue((e.target as HTMLSelectElement).value))}
			>
				<option value="">Default</option>
				<option value="on">On</option>
				<option value="off">Off</option>
			</select>
		</ToolField>
		<Button onclick={handleFind} disabled={!filePath.trim() || loading}>
			{#if loading}Searching...{:else}Find References{/if}
		</Button>
	</div>

	{#if result}
		<div class="result-head">
			<Badge label={`${result.total_count} references`} variant="info" />
			<Badge label={`${result.file_count} files`} variant="default" />
		</div>
		{#each result.references as group (group.path)}
			<div class="ref-group">
				<div class="ref-head">
					<span class="mono path">{group.path}</span>
					<Badge label={String(group.count)} variant="default" />
				</div>
				<ul class="ref-list">
					{#each group.references as ref, i (i)}
						<li class="ref-item">
							<span class="mono">
								L{ref.line}{ref.end_line && ref.end_line !== ref.line
									? `-${ref.end_line}`
									: ''}:C{ref.column}
							</span>
							{#if ref.caller_entity}
								<span class="ref-caller">in {ref.caller_entity}</span>
							{/if}
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	{/if}
</ToolShell>

<style>
	.result-head {
		display: flex;
		gap: 0.5rem;
	}

	.ref-group {
		border: 1px solid var(--gray-200);
	}

	.ref-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		padding: 0.5rem 0.75rem;
		background: var(--gray-50);
		border-bottom: 1px solid var(--gray-200);
	}

	.path {
		word-break: break-all;
	}

	.ref-list {
		list-style: none;
		margin: 0;
		padding: 0.35rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.ref-item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.78rem;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}

	.ref-caller {
		color: var(--gray-500);
		font-size: 0.75rem;
	}
</style>
