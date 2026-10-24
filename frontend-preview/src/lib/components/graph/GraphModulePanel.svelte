<script lang="ts">
	/**
	 * Module-level relations for one file: exports, caller files and
	 * outgoing module edges. Page-local results driven by the page's module
	 * query (shared with the seed bar's Module button), so state arrives as
	 * props and actions as callbacks.
	 */
	import type { GraphModuleResponse } from '$lib/api/graph';

	interface Props {
		data: GraphModuleResponse | null;
		loading?: boolean;
		error: string | null;
		stale: boolean;
		watchEpoch: number | null;
		onFocusEntity: (entityId: string) => void;
		onImpactFile: (file: string) => void;
	}

	let {
		data,
		loading = false,
		error,
		stale,
		watchEpoch,
		onFocusEntity,
		onImpactFile,
	}: Props = $props();
</script>

<section class="panel-section-block" aria-label="Module relations">
	<div class="panel-head">
		<h4 class="communities-title">Module relations</h4>
		{#if stale}
			<span class="stale-tag">Stale — index advanced to {watchEpoch}</span>
		{/if}
	</div>
	{#if error}
		<p class="panel-error">{error}</p>
	{:else if loading && data === null}
		<p class="panel-empty">Loading module relations…</p>
	{:else if data !== null}
		{#if loading}
			<p class="panel-empty">Refreshing module relations…</p>
		{/if}
		<p class="communities-hint">
			File <span class="mono">{data.file}</span> — exports, caller files and outgoing
			module edges.
		</p>
		<div class="module-groups">
			<div class="module-group">
				<h5 class="subsection-title">Exports ({data.exports.length})</h5>
				{#if data.exports.length === 0}
					<p class="panel-empty">No exported symbols for this file.</p>
				{:else}
					<ul class="chip-list">
						{#each data.exports as symbol (symbol)}
							<li>
								<button
									type="button"
									class="chip-btn mono"
									onclick={() => onFocusEntity(symbol)}
									title="Load neighborhood for this symbol"
								>
									{symbol}
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
			<div class="module-group">
				<h5 class="subsection-title">
					Caller files ({data.caller_files.length})
				</h5>
				{#if data.caller_files.length === 0}
					<p class="panel-empty">No caller files target this file.</p>
				{:else}
					<ul class="chip-list">
						{#each data.caller_files as file (file)}
							<li>
								<button
									type="button"
									class="chip-btn mono"
									onclick={() => onImpactFile(file)}
									title="Run file impact analysis for this file"
								>
									{file}
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
			<div class="module-group">
				<h5 class="subsection-title">Module edges ({data.imports.length})</h5>
				{#if data.imports.length === 0}
					<p class="panel-empty">No outgoing module edges.</p>
				{:else}
					<ul class="module-edge-list">
						{#each data.imports as edge, index (index)}
							<li class="module-edge-item">
								<span class="mono">{edge.target}</span>
								<span class="relation-meta"
									>{edge.relation} · {edge.domain}</span
								>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		</div>
	{:else}
		<p class="panel-empty">
			Not queried yet — enter a file path above and press Module.
		</p>
	{/if}
</section>

<style>
	.panel-section-block {
		margin-top: 1rem;
		padding: 1rem;
		border: 1px solid var(--black);
	}

	.panel-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.25rem;
	}

	.communities-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-600);
		margin: 0 0 0.25rem;
	}

	.stale-tag {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--warning);
		border: 1px solid var(--warning);
		padding: 0.1rem 0.4rem;
	}

	.panel-error {
		font-size: 0.75rem;
		color: var(--danger);
		margin: 0;
	}

	.panel-empty {
		font-size: 0.78rem;
		color: var(--gray-500);
		font-style: italic;
		margin: 0;
	}

	.communities-hint {
		font-size: 0.7rem;
		color: var(--gray-500);
		margin: 0 0 0.75rem;
	}

	.subsection-title {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		font-weight: 400;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
		margin: 0 0 0.5rem;
	}

	.module-groups {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 1rem;
	}

	.chip-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		margin: 0;
		padding: 0;
		max-height: 260px;
		overflow-y: auto;
	}

	.chip-btn {
		width: 100%;
		padding: 0.3rem 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		text-align: left;
		word-break: break-all;
		transition: all 0.15s;
	}

	.chip-btn:hover {
		border-color: var(--black);
		background: var(--gray-100);
	}

	.module-edge-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin: 0;
		padding: 0;
		max-height: 260px;
		overflow-y: auto;
	}

	.module-edge-item {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		border-left: 2px solid var(--gray-200);
		padding-left: 0.5rem;
		word-break: break-all;
	}

	.relation-meta {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		color: var(--gray-500);
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}
</style>
