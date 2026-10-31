<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		htmlFor?: string;
		wide?: boolean;
		children?: Snippet;
	}

	let { label, htmlFor = '', wide = false, children }: Props = $props();
</script>

<label
	class="tool-field"
	class:tool-field-wide={wide}
	for={htmlFor || undefined}
>
	<span class="tool-field-label">{label}</span>
	{@render children?.()}
</label>

<style>
	.tool-field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		width: 110px;
	}

	.tool-field-wide {
		flex: 1;
		min-width: 220px;
		width: auto;
	}

	.tool-field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}
</style>
