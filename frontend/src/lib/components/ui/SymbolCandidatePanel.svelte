<script lang="ts">
	/**
	 * Disambiguation candidates shared by every surface that may receive an
	 * AMBIGUOUS_SYMBOL error. Picking one hands the stable id back to the
	 * caller, which decides whether to retry, navigate or re-query.
	 */
	import type { SymbolCandidate } from '$lib/api/client';
	import SelectChip from '$lib/components/ui/SelectChip.svelte';

	interface Props {
		candidates: SymbolCandidate[];
		onPick: (candidate: SymbolCandidate) => void;
	}

	let { candidates, onPick }: Props = $props();
</script>

<div class="candidate-panel" aria-label="Symbol candidates">
	<p class="candidate-hint">Multiple symbols match this name — pick one:</p>
	<p class="candidate-context">
		The current view stays loaded until a candidate is chosen.
	</p>
	<ul class="candidate-list">
		{#each candidates as candidate (candidate.stable_id)}
			<li>
				<SelectChip onclick={() => onPick(candidate)}>
					<span class="candidate-text">
						<span class="candidate-name font-mono">{candidate.scoped_name}</span>
						<span class="candidate-meta"
							>{candidate.kind} · {candidate.file_path}</span
						>
					</span>
				</SelectChip>
			</li>
		{/each}
	</ul>
</div>

<style>
	.candidate-panel {
		padding: 0.75rem;
		margin-bottom: 1rem;
		border: var(--border-soft);
	}

	.candidate-context {
		margin: 0 0 0.5rem;
		font-size: 0.7rem;
		color: var(--gray-500);
	}

	.candidate-hint {
		margin: 0 0 0.5rem;
		font-family: var(--font-mono);
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--gray-500);
	}

	.candidate-list {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 0.5rem;
		margin: 0;
		padding: 0;
	}

	.candidate-text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.candidate-name {
		color: var(--black);
	}

	.candidate-meta {
		font-family: var(--font-mono);
		font-size: 0.6rem;
		color: var(--gray-500);
		word-break: break-all;
	}
</style>
