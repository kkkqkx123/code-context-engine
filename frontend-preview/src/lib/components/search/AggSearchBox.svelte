<script lang="ts">
	import { searchApi, type SubQuery } from '$lib/api/search';

	interface Props {
		placeholder?: string;
		onSearch?: (results: any) => void;
	}

	let {
		placeholder = 'Enter a search description...',
		onSearch = () => {}
	}: Props = $props();

	let originalQuery = $state('');
	let bm25Query = $state('');
	let vectorQuery = $state('');
	let limit = $state(10);
	let showAdvanced = $state(false);
	let isSearching = $state(false);
	let error = $state('');

	async function handleSearch() {
		if (!originalQuery.trim()) return;

		isSearching = true;
		error = '';

		try {
			// Build sub-queries
			const subQueries: SubQuery[] = [];

			if (bm25Query.trim()) {
				subQueries.push({
					text: bm25Query.trim(),
					query_type: 'bm25',
					weight: 1.2
				});
			}

			if (vectorQuery.trim()) {
				subQueries.push({
					text: vectorQuery.trim(),
					query_type: 'vector',
					weight: 1.0
				});
			}

			// If user didn't specify sub-queries, auto-decompose
			if (subQueries.length === 0) {
				subQueries.push(
					{ text: originalQuery, query_type: 'bm25', weight: 1.2 },
					{ text: originalQuery, query_type: 'vector', weight: 1.0 }
				);
			}

			const response = await searchApi.aggregatedSearch({
				project_id: 1, // TODO: Get from context or config
				sub_queries: subQueries,
				limit
			});

			// Call the search callback with results
			onSearch(response);
		} catch (err) {
			console.error('Search failed:', err);
			error = err instanceof Error ? err.message : '搜索失败';
		} finally {
			isSearching = false;
		}
	}
</script>

<div class="advanced-search-box">
	<div class="main-input">
		<input
			type="text"
			bind:value={originalQuery}
			{placeholder}
			onkeydown={(e) => e.key === 'Enter' && handleSearch()}
		/>
		<button onclick={handleSearch} disabled={!originalQuery.trim() || isSearching}>
			{#if isSearching}搜索中...{:else}搜索{/if}
		</button>
		<button class="toggle-advanced" onclick={() => showAdvanced = !showAdvanced}>
			{showAdvanced ? '▲' : '▼'} 高级选项
		</button>
	</div>

	{#if error}
		<div class="error-message">{error}</div>
	{/if}

	{#if showAdvanced}
		<div class="advanced-options">
			<div class="query-section">
				<label>
					BM25 关键词（精确匹配）
					<input
						type="text"
						bind:value={bm25Query}
						placeholder="例如: authenticate login verify"
					/>
				</label>

				<label>
					Vector 语义（理解意图）
					<input
						type="text"
						bind:value={vectorQuery}
						placeholder="例如: user authentication identity"
					/>
				</label>
			</div>

			<div class="options-row">
				<label>
					结果数量
					<input type="number" bind:value={limit} min="1" max="50" />
				</label>
			</div>

			<div class="tips">
				💡 提示：留空将自动使用原始查询进行 BM25 + Vector 搜索
			</div>
		</div>
	{/if}
</div>

<style>
	.advanced-search-box {
		width: 100%;
		max-width: 800px;
		margin: 0 auto;
	}

	.main-input {
		display: flex;
		gap: 0.5rem;
		align-items: stretch;
	}

	.main-input input {
		flex: 1;
		padding: 0.75rem 1rem;
		border: 1px solid var(--black);
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		background: var(--white);
		color: var(--black);
		outline: none;
		transition: border-color 0.3s;
	}

	.main-input input:focus {
		border-color: var(--accent);
	}

	.main-input button {
		padding: 0.75rem 1.5rem;
		background: var(--black);
		color: var(--white);
		border: 1px solid var(--black);
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		transition: background 0.3s;
	}

	.main-input button:hover:not(:disabled) {
		background: var(--accent);
	}

	.main-input button:disabled {
		background: var(--gray-300);
		border-color: var(--gray-300);
		color: var(--gray-700);
		cursor: not-allowed;
	}

	.toggle-advanced {
		background: var(--white);
		color: var(--gray-600);
		border: 1px solid var(--gray-300);
		padding: 0.5rem 1rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.toggle-advanced:hover {
		border-color: var(--accent);
		color: var(--black);
	}

	.error-message {
		margin-top: 0.75rem;
		padding: 1rem;
		background: var(--danger-bg);
		border-left: 4px solid var(--danger);
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.advanced-options {
		margin-top: 1rem;
		padding: 1rem;
		background: var(--gray-100);
		border: 1px solid var(--gray-200);
	}

	.query-section {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.query-section label {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.query-section input {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Grotesk', sans-serif;
		font-size: 0.9rem;
		background: var(--white);
		color: var(--black);
		outline: none;
		transition: border-color 0.3s;
	}

	.query-section input:focus {
		border-color: var(--accent);
	}

	.options-row {
		display: flex;
		gap: 1rem;
		align-items: center;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.options-row input {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--gray-300);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		background: var(--white);
		color: var(--black);
		outline: none;
		transition: border-color 0.3s;
		width: 80px;
	}

	.options-row input:focus {
		border-color: var(--accent);
	}

	.tips {
		margin-top: 0.75rem;
		padding: 0.75rem 1rem;
		background: var(--info-bg);
		border-left: 2px solid var(--info);
		font-family: 'Space Grotesk', sans-serif;
		font-size: 0.85rem;
		color: var(--gray-700);
	}

	@media (max-width: 768px) {
		.main-input {
			flex-wrap: wrap;
		}

		.main-input input {
			flex-basis: 100%;
		}

		.query-section {
			grid-template-columns: 1fr;
		}
	}
</style>
