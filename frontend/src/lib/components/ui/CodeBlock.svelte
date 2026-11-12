<script lang="ts">
	import { tokenizeLine } from '$lib/utils/code';

	interface Props {
		code?: string;
		language?: string;
		showLineNumbers?: boolean;
		/** Line number of the first rendered line (1-based). */
		startLine?: number;
		/** Query terms to mark inside the code. */
		highlightTerms?: string[];
		/** Absolute line numbers to emphasize (e.g. a search hit). */
		highlightLines?: number[];
		/** When set, each line gets id `${prefix}${lineNo}` for anchors. */
		lineIdPrefix?: string;
	}

	let {
		code = '',
		language = '',
		showLineNumbers = false,
		startLine = 1,
		highlightTerms = [],
		highlightLines = [],
		lineIdPrefix = '',
	}: Props = $props();

	let lines = $derived(code.split('\n'));
	let marked = $derived(new Set(highlightLines));
</script>

<div class="code-block">
	{#if language}
		<div class="code-header">
			<span class="code-language">{language}</span>
		</div>
	{/if}
	<!-- Whitespace-sensitive <pre>: prettier keeps line breaks inside tags only. -->
	<pre><code class="code-content"
			>{#each lines as line, i (i)}<span
					class="code-line"
					class:marked={marked.has(startLine + i)}
					id={lineIdPrefix ? `${lineIdPrefix}${startLine + i}` : undefined}
					>{#if showLineNumbers}<span class="line-no" aria-hidden="true"
							>{startLine + i}</span
						>{/if}<span class="line-text"
						>{#each tokenizeLine(line, highlightTerms) as token, j (j)}<span
								class={token.cls}>{token.text}</span
							>{/each}</span
					></span
				>{/each}</code
		></pre>
</div>

<style>
	.code-block {
		background: var(--black);
		color: var(--white);
		border: var(--border-strong);
		overflow: hidden;
	}

	.code-header {
		padding: 0.75rem 1rem;
		background: var(--gray-800);
		border-bottom: 1px solid var(--gray-600);
	}

	.code-language {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-widest);
		color: var(--gray-400);
	}

	pre {
		padding: 1.5rem;
		margin: 0;
		overflow-x: auto;
		line-height: 1.8;
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.85rem;
		background: transparent;
		padding: 0;
		color: var(--white);
	}

	.code-line {
		display: block;
		min-height: 1.8em;
	}

	.code-line.marked {
		background: rgba(230, 54, 0, 0.22);
	}

	.line-no {
		display: inline-block;
		width: 3ch;
		margin-right: 1.5ch;
		text-align: right;
		color: var(--gray-400);
		user-select: none;
	}

	.line-text {
		white-space: pre;
	}
</style>
