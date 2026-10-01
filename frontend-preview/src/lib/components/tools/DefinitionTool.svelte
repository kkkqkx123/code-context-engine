<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { toolsApi, type GotoDefinitionResult } from '$lib/api/tools';
	import { currentProjectId } from '$lib/stores/project';
	import { get } from 'svelte/store';

	let filePath = $state('');
	let line = $state(1);
	let column = $state(1);
	let includeBody = $state(true);
	let result = $state<GotoDefinitionResult | null>(null);
	let loading = $state(false);
	let error = $state('');

	async function handleGoto() {
		const path = filePath.trim();
		if (!path) return;
		loading = true;
		error = '';
		result = null;
		try {
			result = await toolsApi.getDefinition({
				project_id: get(currentProjectId),
				path,
				line,
				column,
				include_body: includeBody
			});
		} catch (e: any) {
			error = e?.message ?? 'Goto definition failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="tool">
	{#if error}
		<div class="tool-error">{error}</div>
	{/if}

	<div class="form-row">
		<label class="field field-wide">
			<span class="field-label">File Path</span>
			<input class="text-input" type="text" bind:value={filePath} placeholder="/path/to/file.ts" />
		</label>
		<label class="field field-narrow">
			<span class="field-label">Line</span>
			<input class="text-input" type="number" min="1" bind:value={line} />
		</label>
		<label class="field field-narrow">
			<span class="field-label">Column</span>
			<input class="text-input" type="number" min="1" bind:value={column} />
		</label>
		<Button onclick={handleGoto} disabled={!filePath.trim() || loading}>
			{#if loading}Resolving...{:else}Goto Definition{/if}
		</Button>
	</div>

	<label class="check-row">
		<input type="checkbox" bind:checked={includeBody} />
		<span>Include definition body</span>
	</label>

	{#if result}
		{#if result.definitions.length === 0}
			<p class="placeholder">No definition found at the given position.</p>
		{:else}
			{#each result.definitions as def, i (i)}
				<div class="def-card">
					<div class="def-head">
						<Badge label={def.kind} variant="info" />
						<span class="def-name">{def.name}</span>
					</div>
					{#if def.signature}
						<pre class="def-signature mono">{def.signature}</pre>
					{/if}
					<p class="def-location mono">
						{def.location.path}:{def.location.line}
						<a class="def-link" href={`/entities/${def.location.entity_id}`}>open entity</a>
					</p>
					{#if def.code}
						<pre class="def-code">{def.code}</pre>
					{/if}
				</div>
			{/each}
		{/if}
	{/if}
</div>

<style>
	.tool {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.tool-error {
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
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

	.check-row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.8rem;
		color: var(--gray-600);
	}

	.def-card {
		border: 1px solid var(--gray-200);
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.def-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}

	.def-name {
		font-weight: 700;
		font-size: 0.9rem;
	}

	.def-signature {
		margin: 0;
		padding: 0.5rem;
		background: var(--gray-50);
		border: 1px solid var(--gray-200);
		overflow-x: auto;
	}

	.def-location {
		margin: 0;
		word-break: break-all;
	}

	.def-link {
		margin-left: 0.75rem;
		color: var(--accent);
		text-decoration: none;
		border-bottom: 1px solid var(--accent);
	}

	.def-code {
		margin: 0;
		padding: 0.75rem;
		background: var(--gray-900);
		color: var(--gray-200);
		font-size: 0.72rem;
		line-height: 1.5;
		overflow-x: auto;
	}

	.placeholder {
		color: var(--gray-400);
		font-style: italic;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}
</style>
