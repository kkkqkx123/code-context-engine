<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import ToolFormRow from '$lib/components/tools/ToolFormRow.svelte';
	import { toolsApi, type GotoDefinitionResult } from '$lib/api/tools';
	import { projectStore } from '$lib/stores/project.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

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
				project_id: projectStore.currentProjectId,
				path,
				line,
				column,
				include_body: includeBody,
			});
		} catch (e) {
			error = errorMessage(e) ?? 'Goto definition failed';
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<ToolFormRow>
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
		<Button onclick={handleGoto} disabled={!filePath.trim() || loading}>
			{#if loading}Resolving...{:else}Goto Definition{/if}
		</Button>
	</ToolFormRow>

	<Toggle
		checked={includeBody}
		label="Include definition body"
		onchange={(e) => (includeBody = e.checked)}
	/>

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
						<a
							class="def-link"
							href={resolve(`/entities/${def.location.entity_id}`)}
							onclick={(e) => {
								e.preventDefault();
								goto(resolve(`/entities/${def.location.entity_id}`));
							}}>open entity</a
						>
					</p>
					{#if def.code}
						<pre class="def-code">{def.code}</pre>
					{/if}
				</div>
			{/each}
		{/if}
	{/if}
</ToolShell>

<style>
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
