<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import ToolShell from '$lib/components/tools/ToolShell.svelte';
	import ToolField from '$lib/components/tools/ToolField.svelte';
	import { toolsApi, type BatchFoldResponse } from '$lib/api/tools';

	interface DraftItem {
		text: string;
		language: string;
		fileName: string;
		maxTokens: number | null;
		mode: string;
	}

	let items = $state<DraftItem[]>([
		{ text: '', language: '', fileName: '', maxTokens: null, mode: '' },
	]);
	let globalLanguage = $state('');
	let globalMaxTokens = $state<number | null>(null);
	let globalMode = $state('');
	let result = $state<BatchFoldResponse | null>(null);
	let loading = $state(false);
	let error = $state('');

	function addItem() {
		items = [
			...items,
			{ text: '', language: '', fileName: '', maxTokens: null, mode: '' },
		];
	}

	function removeItem(index: number) {
		items = items.filter((_, i) => i !== index);
	}

	async function handleFold() {
		const payload = items
			.map((item, index) => ({
				id: String(index),
				text: item.text,
				language: item.language.trim() || undefined,
				file_name: item.fileName.trim() || undefined,
				max_tokens: item.maxTokens ?? undefined,
				mode: item.mode.trim() || undefined,
			}))
			.filter((item) => item.text.trim());
		if (payload.length === 0) return;
		loading = true;
		error = '';
		result = null;
		try {
			result = await toolsApi.foldBatch({
				items: payload,
				language: globalLanguage.trim() || undefined,
				max_tokens: globalMaxTokens ?? undefined,
				mode: globalMode.trim() || undefined,
			});
		} catch (e) {
			error = errorMessage(e) ?? 'Batch fold failed';
		} finally {
			loading = false;
		}
	}
</script>

<ToolShell {error}>
	<div class="tool-form-row">
		<ToolField label="Global language (optional)" wide>
			<input
				class="text-input"
				type="text"
				bind:value={globalLanguage}
				placeholder="e.g., rust"
			/>
		</ToolField>
		<ToolField label="Global max tokens (optional)">
			<input
				class="text-input"
				type="number"
				min="1"
				value={globalMaxTokens ?? ''}
				oninput={(e) => {
					const n = Number((e.target as HTMLInputElement).value);
					globalMaxTokens = Number.isInteger(n) && n > 0 ? n : null;
				}}
			/>
		</ToolField>
		<ToolField label="Global mode (optional)">
			<input
				class="text-input"
				type="text"
				bind:value={globalMode}
				placeholder="detailed or minimal"
			/>
		</ToolField>
	</div>

	{#each items as item, i (i)}
		<div class="item">
			<div class="item-head">
				<span class="mono">Item {i + 1}</span>
				<Button
					variant="ghost"
					size="xs"
					onclick={() => removeItem(i)}
					disabled={items.length <= 1}
					aria-label={`Remove item ${i + 1}`}
				>
					<Icon name="close" />
				</Button>
			</div>
			<textarea
				class="text-input"
				rows="4"
				bind:value={item.text}
				placeholder="Raw source text"></textarea>
			<div class="tool-form-row">
				<ToolField label="Language (optional)" wide>
					<input
						class="text-input"
						type="text"
						bind:value={item.language}
						placeholder="e.g., python"
					/>
				</ToolField>
				<ToolField label="File name (optional)" wide>
					<input
						class="text-input"
						type="text"
						bind:value={item.fileName}
						placeholder="e.g., main.py"
					/>
				</ToolField>
				<ToolField label="Max tokens (optional)">
					<input
						class="text-input"
						type="number"
						min="1"
						value={item.maxTokens ?? ''}
						oninput={(e) => {
							const n = Number((e.target as HTMLInputElement).value);
							item.maxTokens = Number.isInteger(n) && n > 0 ? n : null;
						}}
					/>
				</ToolField>
				<ToolField label="Mode (optional)">
					<input
						class="text-input"
						type="text"
						bind:value={item.mode}
						placeholder="detailed or minimal"
					/>
				</ToolField>
			</div>
		</div>
	{/each}

	<div class="tool-actions">
		<Button variant="secondary" onclick={addItem}>+ Add item</Button>
		<Button
			onclick={handleFold}
			disabled={loading || !items.some((item) => item.text.trim())}
		>
			{#if loading}Folding...{:else}Fold batch{/if}
		</Button>
	</div>

	{#if result}
		<div class="result-head">
			<Badge
				label={`${result.results.length} folded · ${result.stats.total_original_tokens} → ${result.stats.total_folded_tokens} tokens`}
				variant="info"
			/>
		</div>
		{#each result.results as entry (entry.id)}
			<div class="hit">
				<div class="hit-head">
					<span class="hit-title">Item {Number(entry.id) + 1}</span>
					<Badge
						label={entry.structure_known ? entry.language : 'unknown structure'}
						variant="default"
					/>
				</div>
				<pre class="folded">{entry.folded_text}</pre>
			</div>
		{/each}
	{/if}
</ToolShell>

<style>
	.item {
		border: 1px solid var(--gray-200);
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.item-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.result-head {
		display: flex;
		gap: 0.5rem;
	}

	.hit {
		border: 1px solid var(--gray-200);
		padding: 0.6rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.hit-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
	}

	.hit-title {
		font-weight: 700;
		font-size: 0.85rem;
	}

	.folded {
		margin: 0;
		white-space: pre-wrap;
		font-size: 0.78rem;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}
</style>
