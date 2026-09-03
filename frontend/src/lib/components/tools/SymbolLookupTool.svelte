<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { toolsApi } from '$lib/api/tools';
	import { currentProjectId } from '$lib/stores/project';
	import type { SymbolInfo } from '$lib/api/tools';

	interface Props {
		filePath?: string;
		language?: string;
	}

	let { filePath = $bindable(''), language = $bindable('typescript') }: Props = $props();

	let result: any = $state(null);
	let loading = $state(false);
	let error: string | null = $state(null);

	async function handleGetSymbols() {
		if (!filePath.trim()) return;

		loading = true;
		error = null;
		result = null;

		try {
			let projectId: number;
			currentProjectId.subscribe(v => projectId = v)();

			result = await toolsApi.getSymbols({ project_id: projectId!, paths: [filePath] });
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

	<div class="symbol-input-section">
		<div class="input-row">
			<div class="input-group">
				<label class="field-label" for="symbol-file-path">File Path</label>
				<Input
					id="symbol-file-path"
					type="text"
					bind:value={filePath}
					placeholder="/path/to/file.ts"
				/>
			</div>

			<div class="input-group">
				<label class="field-label" for="symbol-language">Language</label>
				<select id="symbol-language" bind:value={language} class="select-input">
					<option value="typescript">TypeScript</option>
					<option value="javascript">JavaScript</option>
					<option value="rust">Rust</option>
					<option value="python">Python</option>
					<option value="go">Go</option>
					<option value="java">Java</option>
				</select>
			</div>

			<div class="input-group input-action">
				<Button
					onclick={handleGetSymbols}
					disabled={!filePath.trim() || loading}
				>
					{#if loading}Extracting...{:else}Extract Symbols{/if}
				</Button>
			</div>
		</div>
	</div>

	{#if result?.result?.results}
		{@const symbols = result.result.results.flatMap((r: any) => r.symbols ?? [])}
		<div class="symbols-results">
			<h3 class="results-title">Found {symbols.length} Symbol(s)</h3>
			<div class="symbols-table">
				<div class="table-header">
					<div class="col-name">Name</div>
					<div class="col-kind">Kind</div>
					<div class="col-location">Location</div>
				</div>
				{#each symbols as sym}
					<div class="table-row">
						<div class="col-name" data-label="Name">{sym.name}</div>
						<div class="col-kind" data-label="Kind">
							<Badge label={sym.kind} variant="default" />
						</div>
						<div class="col-location" data-label="Location">Lines {sym.line}-{sym.end_line}</div>
					</div>
				{/each}
			</div>
		</div>
	{:else if !loading}
		<div class="empty-output">Symbol extraction results will appear here...</div>
	{/if}
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

	.symbol-input-section {
		margin-bottom: 2rem;
	}

	.input-row {
		display: grid;
		grid-template-columns: 2fr 1fr auto;
		gap: 1rem;
		align-items: end;
	}

	.input-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.input-action {
		min-width: 150px;
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

	.empty-output {
		padding: 3rem;
		text-align: center;
		color: var(--gray-500);
		font-style: italic;
	}

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

	.symbols-table {
		border: 1px solid var(--gray-200);
	}

	.table-header,
	.table-row {
		display: grid;
		grid-template-columns: 2fr 1fr 1fr;
		gap: 1rem;
		padding: 0.75rem 1rem;
		align-items: center;
	}

	.table-header {
		background: var(--gray-100);
		border-bottom: 1px solid var(--gray-200);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	.table-row {
		border-bottom: 1px solid var(--gray-200);
		transition: background 0.2s ease;
	}

	.table-row:last-child {
		border-bottom: none;
	}

	.table-row:hover {
		background: var(--gray-100);
	}

	.col-name {
		font-weight: 600;
		color: var(--black);
	}

	.col-kind {
		text-align: center;
	}

	.col-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		color: var(--gray-600);
	}

	@media (max-width: 1024px) {
		.input-row {
			grid-template-columns: 1fr;
		}

		.table-header,
		.table-row {
			grid-template-columns: 1fr;
			gap: 0.5rem;
		}

		.table-header > div,
		.table-row > div {
			padding: 0.25rem 0;
		}
	}

	@media (max-width: 480px) {
		.table-header {
			display: none;
		}

		.table-row {
			display: block;
			padding: 1rem;
			margin-bottom: 0.75rem;
			border: 1px solid var(--gray-200);
			background: var(--white);
		}

		.table-row > div {
			padding: 0.25rem 0;
			position: relative;
			padding-left: 40%;
		}

		.table-row > div::before {
			content: attr(data-label);
			position: absolute;
			left: 0;
			width: 35%;
			font-family: 'Space Mono', monospace;
			font-size: 0.7rem;
			text-transform: uppercase;
			color: var(--gray-600);
		}

		.col-name,
		.col-kind,
		.col-location {
			text-align: left;
		}
	}
</style>
