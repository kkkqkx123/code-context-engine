<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import ToolFormRow from '$lib/components/tools/ToolFormRow.svelte';
	import { toolsApi } from '$lib/api/tools';
	import { projectStore } from '$lib/stores/project.svelte';
	import type { GetSymbolsResult, FileSymbolResult } from '$lib/api/tools';

	interface Props {
		filePath?: string;
		language?: string;
	}

	let { filePath = $bindable(''), language = $bindable('typescript') }: Props =
		$props();

	let result: GetSymbolsResult | null = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleGetSymbols() {
		if (!filePath.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			result = await toolsApi.getSymbols({
				project_id: projectStore.currentProjectId,
				paths: [filePath],
			});
		} catch (err) {
			error = errorMessage(err);
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<ToolFormRow>
		<ToolField label="File Path" wide>
			<input
				id="symbol-file-path"
				class="text-input"
				type="text"
				bind:value={filePath}
				placeholder="/path/to/file.ts"
			/>
		</ToolField>

		<ToolField label="Language">
			<select id="symbol-language" class="text-input" bind:value={language}>
				<option value="typescript">TypeScript</option>
				<option value="javascript">JavaScript</option>
				<option value="rust">Rust</option>
				<option value="python">Python</option>
				<option value="go">Go</option>
				<option value="java">Java</option>
			</select>
		</ToolField>

		<Button onclick={handleGetSymbols} disabled={!filePath.trim() || loading}>
			{#if loading}Extracting...{:else}Extract Symbols{/if}
		</Button>
	</ToolFormRow>

	{#if result?.results}
		{@const symbols = result.results.flatMap(
			(r: FileSymbolResult) => r.symbols ?? [],
		)}
		<div class="symbols-results">
			<h3 class="results-title">Found {symbols.length} Symbol(s)</h3>
			<div class="table-wrap">
				<table class="table">
					<thead>
						<tr>
							<th>Name</th>
							<th>Kind</th>
							<th>Location</th>
						</tr>
					</thead>
					<tbody>
						{#each symbols as sym (sym.name)}
							<tr>
								<td class="sym-name">{sym.name}</td>
								<td><Badge label={sym.kind} variant="default" /></td>
								<td class="mono">Lines {sym.line}-{sym.end_line}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</div>
	{:else if !loading}
		<EmptyState
			padded
			message="Symbol extraction results will appear here..."
		/>
	{/if}
</ToolShell>

<style>
	.symbols-results {
		margin-top: 1.5rem;
	}

	.results-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
		margin-bottom: 1rem;
	}

	.sym-name {
		font-weight: 600;
		color: var(--black);
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
	}
</style>
