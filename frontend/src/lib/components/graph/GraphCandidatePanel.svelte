<script lang="ts">
	/**
	 * Disambiguation candidates shown when a symbol-name seed matches
	 * multiple entities (backend AMBIGUOUS_SYMBOL). Picking one lets the
	 * page retry with its stable id.
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
		The current graph stays loaded until a candidate is chosen.
	</p>
	<ul class="candidate-list">
		{#each candidates as candidate (candidate.stable_id)}
			<li>
				<SelectChip onclick={() => onPick(candidate)}>
					<span class="candidate-text">
						<span class="candidate-name mono">{candidate.scoped_name}</span>
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
		padding: 0.6rem 0.75rem;
		margin-bottom: 1rem;
		border: 1px solid var(--gray-300);
	}

	.candidate-context {
		margin: 0 0 0.5rem;
		font-size: 0.7rem;
		color: var(--gray-500);
	}

	.candidate-hint {
		margin: 0 0 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--gray-500);
	}

	.candidate-list {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 0.4rem;
		margin: 0;
		padding: 0;
	}

	.candidate-text {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.candidate-name {
		color: var(--black);
	}

	.candidate-meta {
		font-family: 'Space Mono', monospace;
		font-size: 0.6rem;
		color: var(--gray-500);
		word-break: break-all;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
	}
</style>
