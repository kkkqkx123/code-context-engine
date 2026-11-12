<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		htmlFor?: string;
		wide?: boolean;
		inline?: boolean;
		children?: Snippet;
	}

	let {
		label,
		htmlFor = '',
		wide = false,
		inline = false,
		children,
	}: Props = $props();
</script>

<label
	class="tool-field"
	class:tool-field-wide={wide}
	class:tool-field-inline={inline}
	for={htmlFor || undefined}
>
	<span class="form-label">{label}</span>
	{@render children?.()}
</label>

<style>
	.tool-field {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		width: 110px;
	}

	.tool-field-wide {
		flex: 1;
		min-width: 220px;
		width: auto;
	}

	.tool-field-inline {
		flex-direction: row;
		align-items: center;
		width: auto;
	}

</style>
