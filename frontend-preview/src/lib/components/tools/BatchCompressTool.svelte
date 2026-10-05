<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
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

<div class="tool">
	{#if error}
		<div class="tool-error">{error}</div>
	{/if}

	<label class="field">
		<span class="field-label">File paths (one per line)</span>
		<textarea
			class="paths-input"
			rows="6"
			bind:value={pathsText}
			placeholder="src/auth/login.ts\nsrc/utils/token.ts"
			spellcheck="false"></textarea>
	</label>

	<div class="options-row">
		<label class="check-row">
			<input type="checkbox" bind:checked={includeEntities} />
			<span>Include entities</span>
		</label>
		<label class="check-row">
			<input type="checkbox" bind:checked={includeGroups} />
			<span>Include groups</span>
		</label>
		<label class="field field-narrow">
			<span class="field-label">Max concurrency</span>
			<input
				class="text-input"
				type="number"
				min="1"
				max="16"
				bind:value={maxConcurrency}
			/>
		</label>
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

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.paths-input {
		padding: 0.6rem 0.7rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
		line-height: 1.5;
		color: var(--black);
		resize: vertical;
	}

	.paths-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.options-row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 1rem;
	}

	.check-row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.8rem;
		color: var(--gray-600);
		padding-bottom: 0.4rem;
	}

	.field-narrow {
		width: 140px;
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
