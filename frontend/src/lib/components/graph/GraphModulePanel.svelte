<script lang="ts">
	/**
	 * Module-level relations for one file: exports, caller files and
	 * outgoing module edges. Page-local results driven by the page's module
	 * query (shared with the seed bar's Module button), so state arrives as
	 * props and actions as callbacks.
	 */
	import type { GraphModuleResponse } from '$lib/api/graph';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import SelectChip from '$lib/components/ui/SelectChip.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';

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

<section class="panel-section" aria-label="Module relations">
	<div class="panel-head">
		<h4 class="panel-title">Module relations</h4>
		{#if stale}
			<Badge
				variant="warning"
				label={'Stale — index advanced to ' + watchEpoch}
			/>
		{/if}
	</div>
	{#if error}
		<InlineError message={error} />
	{:else if loading && data === null}
		<p class="empty-hint empty-hint--compact">Loading module relations…</p>
	{:else if data !== null}
		{#if loading}
			<p class="empty-hint empty-hint--compact">Refreshing module relations…</p>
		{/if}
		<p class="panel-hint">
			File <span class="font-mono">{data.file}</span> — exports, caller files and outgoing
			module edges.
		</p>
		<div class="module-groups">
			<div class="module-group">
				<h5 class="eyebrow-title">Exports ({data.exports.length})</h5>
				{#if data.exports.length === 0}
					<p class="empty-hint empty-hint--compact">No exported symbols for this file.</p>
				{:else}
					<ul class="chip-list">
						{#each data.exports as symbol (symbol)}
							<li>
								<SelectChip
									onclick={() => onFocusEntity(symbol)}
									title="Load neighborhood for this symbol"
								>
									<span class="font-mono">{symbol}</span>
								</SelectChip>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
			<div class="module-group">
				<h5 class="eyebrow-title">
					Caller files ({data.caller_files.length})
				</h5>
				{#if data.caller_files.length === 0}
					<p class="empty-hint empty-hint--compact">No caller files target this file.</p>
				{:else}
					<ul class="chip-list">
						{#each data.caller_files as file (file)}
							<li>
								<SelectChip
									onclick={() => onImpactFile(file)}
									title="Run file impact analysis for this file"
								>
									<span class="font-mono">{file}</span>
								</SelectChip>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
			<div class="module-group">
				<h5 class="eyebrow-title">Module edges ({data.imports.length})</h5>
				{#if data.imports.length === 0}
					<p class="empty-hint empty-hint--compact">No outgoing module edges.</p>
				{:else}
					<ul class="module-edge-list">
						{#each data.imports as edge, index (index)}
							<li class="module-edge-item">
								<span class="font-mono">{edge.target}</span>
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
		<p class="empty-hint empty-hint--compact">
			Not queried yet — enter a file path above and press Module.
		</p>
	{/if}
</section>

<style>






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
		font-family: var(--font-mono);
		font-size: 0.6rem;
		color: var(--gray-500);
	}

</style>
