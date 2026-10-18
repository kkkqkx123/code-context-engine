<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import { toolsApi, type FindReferencesResult } from '$lib/api/tools';
	import { currentProjectId } from '$lib/stores/project';
	import { get } from 'svelte/store';

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
				project_id: get(currentProjectId),
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

<div class="tool">
	{#if error}
		<InlineError message={error} />
	{/if}

	<div class="form-row">
		<label class="field field-wide">
			<span class="field-label">File Path</span>
			<input
				class="text-input"
				type="text"
				bind:value={filePath}
				placeholder="/path/to/file.ts"
			/>
		</label>
		<label class="field field-narrow">
			<span class="field-label">Line</span>
			<input class="text-input" type="number" min="1" bind:value={line} />
		</label>
		<label class="field field-narrow">
			<span class="field-label">Column</span>
			<input class="text-input" type="number" min="1" bind:value={column} />
		</label>
		<label class="field field-narrow">
			<span class="field-label">Symbol (optional)</span>
			<input
				class="text-input"
				type="text"
				bind:value={symbol}
				placeholder="authenticate"
			/>
		</label>
		<label class="field field-narrow">
			<span class="field-label">Context lines</span>
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
		</label>
		<label class="field field-narrow">
			<span class="field-label">Snippets</span>
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
		</label>
		<label class="field field-narrow">
			<span class="field-label">Entity info</span>
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
		</label>
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
</div>

<style>
	.tool {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}


	.form-row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 0.75rem;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.field-wide {
		flex: 1;
		min-width: 220px;
	}

	.field-narrow {
		width: 110px;
	}

	.field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.text-input {
		padding: 0.5rem 0.6rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
		color: var(--black);
		width: 100%;
	}

	.text-input:focus {
		outline: none;
		border-color: var(--accent);
	}

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
