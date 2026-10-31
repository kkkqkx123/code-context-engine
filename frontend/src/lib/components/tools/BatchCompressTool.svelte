<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import { toolsApi, type BatchCompressResponse } from '$lib/api/tools';

	let pathsText = $state('');
	let includeEntities = $state(true);
	let includeGroups = $state(false);
	let maxConcurrency = $state(4);
	let result = $state<BatchCompressResponse | null>(null);
	let loading = $state(false);
	let error = $state('');

	async function handleBatch() {
		const paths = pathsText
			.split('\n')
			.map((line) => line.trim())
			.filter((line) => line.length > 0);
		if (paths.length === 0) return;
		loading = true;
		error = '';
		result = null;
		try {
			result = await toolsApi.batchCompress({
				file_paths: paths,
				include_entities: includeEntities,
				include_groups: includeGroups,
				max_concurrency: maxConcurrency,
			});
		} catch (e) {
			error = errorMessage(e) ?? 'Batch compress failed';
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<ToolField label="File paths (one per line)" wide>
		<textarea
			class="text-input"
			rows="6"
			bind:value={pathsText}
			placeholder="src/auth/login.ts\nsrc/utils/token.ts"
			spellcheck="false"></textarea>
	</ToolField>

	<div class="tool-form-row">
		<Toggle
			checked={includeEntities}
			label="Include entities"
			onchange={(e) => (includeEntities = e.checked)}
		/>
		<Toggle
			checked={includeGroups}
			label="Include groups"
			onchange={(e) => (includeGroups = e.checked)}
		/>
		<ToolField label="Max concurrency">
			<input
				class="text-input"
				type="number"
				min="1"
				max="16"
				bind:value={maxConcurrency}
			/>
		</ToolField>
		<Button onclick={handleBatch} disabled={loading || !pathsText.trim()}>
			{#if loading}Compressing...{:else}Batch Compress{/if}
		</Button>
	</div>

	{#if result}
		<div class="result-head">
			<Badge label={`${result.successes.length} ok`} variant="success" />
			<Badge
				label={`${result.failures.length} failed`}
				variant={result.failures.length > 0 ? 'danger' : 'default'}
			/>
		</div>

		{#each result.successes as item (item.path)}
			<div class="entry">
				<div class="entry-head">
					<span class="mono path">{item.path}</span>
					<Badge
						label={item.result.from_cache ? 'cached' : 'fresh'}
						variant="info"
					/>
				</div>
				<p class="entry-detail mono">
					{item.result.language} · {item.result.semantic_text.length} chars semantic
					text
				</p>
			</div>
		{/each}

		{#each result.failures as failure (failure.path)}
			<div class="entry entry-failed">
				<div class="entry-head">
					<span class="mono path">{failure.path}</span>
					<Badge label="failed" variant="danger" />
				</div>
				<p class="entry-error mono">{failure.error}</p>
			</div>
		{/each}
	{/if}
</ToolShell>

<style>
	.result-head {
		display: flex;
		gap: 0.5rem;
	}

	.entry {
		border: 1px solid var(--gray-200);
		padding: 0.6rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.entry-failed {
		border-color: var(--danger);
	}

	.entry-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
	}

	.entry-detail {
		margin: 0;
	}

	.entry-error {
		margin: 0;
		color: var(--danger);
		word-break: break-all;
	}

	.path {
		word-break: break-all;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}
</style>
