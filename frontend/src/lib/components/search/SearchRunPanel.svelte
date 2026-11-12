<script lang="ts">
	/**
	 * Retrieval run snapshot (stage 3).
	 *
	 * Purely client-side: it only reads fields the backend already returns
	 * (sources_used, elapsed_ms, failed_sub_queries, relation_epoch, score,
	 * sources) plus the local filter state. No LLM and no new API call.
	 */
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { searchStore } from '$lib/stores/search.svelte';

	let open = $state(false);
	let copied = $state(false);
	let baseline = $state<{ label: string; keys: string[] } | null>(null);

	function topKeys(): string[] {
		return searchStore.results
			.slice(0, 10)
			.map((result) => `${result.file_path}:${result.start_line}`);
	}

	function pinBaseline() {
		baseline = { label: searchStore.query, keys: topKeys() };
	}

	let diff = $derived.by(() => {
		if (!baseline) return null;
		const current = new Set(topKeys());
		const base = new Set(baseline.keys);
		return {
			added: [...current].filter((key) => !base.has(key)),
			removed: baseline.keys.filter((key) => !current.has(key)),
			kept: [...current].filter((key) => base.has(key)),
		};
	});

	let filterEntries = $derived(
		Object.entries(searchStore.filters).filter(
			([, value]) =>
				value !== '' && value !== null && value !== 0 && value !== false,
		),
	);

	async function copyLink() {
		try {
			await navigator.clipboard.writeText(window.location.href);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			// Clipboard may be unavailable; the URL stays visible in the address bar.
		}
	}
</script>

<div class="run-panel">
	<div class="run-toggle">
		<Button
			variant="secondary"
			size="xs"
			active={open}
			onclick={() => (open = !open)}
		>
			{open ? 'Hide run details' : 'Show run details'}
		</Button>
		{#if searchStore.elapsedMs !== null}
			<span class="run-meta">
				{searchStore.elapsedMs}ms · {searchStore.total} hits
			</span>
		{/if}
	</div>

	{#if open}
		<div class="run-body">
			<div class="run-grid">
				<div>
					<h3>Run snapshot</h3>
					<ul>
						<li>query: <code>{searchStore.query || '—'}</code></li>
						<li>mode: <Badge variant="default">{searchStore.mode}</Badge></li>
						<li>sources: {searchStore.sourcesUsed.join(', ') || '—'}</li>
						<li>elapsed: {searchStore.elapsedMs ?? '—'} ms</li>
						<li>
							total: {searchStore.total}; loaded: {searchStore.results.length}
						</li>
						<li>
							relation epoch: {searchStore.relationEpoch ??
								'—'}{searchStore.relationStale ? ' (stale)' : ''}
						</li>
						<li>stale: {searchStore.stale ? 'yes' : 'no'}</li>
						{#if searchStore.failedSubQueries.length > 0}
							<li>
								failed sub-queries: {searchStore.failedSubQueries.join(', ')}
							</li>
						{/if}
					</ul>
				</div>
				<div>
					<h3>Active filters</h3>
					{#if filterEntries.length === 0}
						<p class="muted">None (server defaults)</p>
					{:else}
						<ul>
							{#each filterEntries as [key, value] (key)}
								<li>{key}: <code>{String(value)}</code></li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>

			<div class="run-actions">
				<Button variant="ghost" size="xs" onclick={copyLink}>
					{copied ? 'Link copied' : 'Copy reproducible link'}
				</Button>
				<Button
					variant="secondary"
					size="xs"
					onclick={pinBaseline}
					disabled={searchStore.results.length === 0}
				>
					Pin top-10 as baseline
				</Button>
				<Button
					variant="ghost"
					size="xs"
					onclick={() => (baseline = null)}
					disabled={!baseline}
				>
					Clear baseline
				</Button>
			</div>

			{#if baseline && diff}
				<div class="diff">
					<h3>Compared with “{baseline.label}”</h3>
					<p>
						+{diff.added.length} new · −{diff.removed.length} gone · {diff.kept
							.length}
						kept (top 10)
					</p>
					{#if diff.added.length > 0}
						<p class="muted">New: {diff.added.slice(0, 3).join(', ')}</p>
					{/if}
					{#if diff.removed.length > 0}
						<p class="muted">Gone: {diff.removed.slice(0, 3).join(', ')}</p>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.run-panel {
		margin-bottom: 1.5rem;
	}

	.run-toggle {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.run-meta {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--gray-500);
	}

	.run-body {
		margin-top: 0.75rem;
		padding: 1rem;
		border: var(--border-muted);
		background: var(--gray-50);
	}

	.run-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 1.5rem;
	}

	h3 {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		margin-bottom: 0.75rem;
	}

	ul {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 0.5rem;
	}

	li,
	p {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--gray-700);
		word-break: break-word;
	}

	code {
		font-family: var(--font-mono);
		color: var(--black);
	}

	.muted {
		color: var(--gray-500);
	}

	.run-actions,
	.diff {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: var(--border-muted);
	}

	.diff {
		display: grid;
		gap: 0.5rem;
	}

	.diff h3 {
		margin: 0;
	}
</style>
