<script lang="ts">
	/**
	 * Disambiguation candidates shown when a symbol-name seed matches
	 * multiple entities (backend AMBIGUOUS_SYMBOL). Picking one lets the
	 * page retry with its stable id.
	 */
	import type { SymbolCandidate } from '$lib/api/client';

	interface Props {
		candidates: SymbolCandidate[];
		onPick: (candidate: SymbolCandidate) => void;
	}

	let { candidates, onPick }: Props = $props();
</script>

<div class="candidate-panel" aria-label="Symbol candidates">
	<p class="candidate-hint">Multiple symbols match this name — pick one:</p>
	<ul class="candidate-list">
		{#each candidates as candidate (candidate.stable_id)}
			<li>
				<button
					type="button"
					class="candidate-btn"
					onclick={() => onPick(candidate)}
				>
					<span class="candidate-name mono">{candidate.scoped_name}</span>
					<span class="candidate-meta"
						>{candidate.kind} · {candidate.file_path}</span
					>
				</button>
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

	.candidate-btn {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		width: 100%;
		padding: 0.4rem 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		text-align: left;
		transition: all 0.15s;
	}

	.candidate-btn:hover {
		border-color: var(--black);
		background: var(--gray-100);
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
