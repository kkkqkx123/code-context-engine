<script lang="ts">
	interface Props {
		code?: string;
		language?: string;
		showLineNumbers?: boolean;
	}

	let {
		code = '',
		language = '',
		showLineNumbers = false
	}: Props = $props();
</script>

<div class="code-block">
	{#if language}
		<div class="code-header">
			<span class="code-language">{language}</span>
		</div>
	{/if}
	<pre><code class="code-content">{code}</code></pre>
</div>

<style>
	.code-block {
		background: var(--black);
		color: var(--white);
		border: 1px solid var(--black);
		overflow: hidden;
	}

	.code-header {
		padding: 0.75rem 1rem;
		background: var(--gray-800);
		border-bottom: 1px solid var(--gray-600);
	}

	.code-language {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-400);
	}

	pre {
		padding: 1.5rem;
		margin: 0;
		overflow-x: auto;
		line-height: 1.8;
	}

	code {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		background: transparent;
		padding: 0;
		color: var(--white);
	}
</style>
