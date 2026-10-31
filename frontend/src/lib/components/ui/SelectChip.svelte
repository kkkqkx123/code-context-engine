<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	interface Props extends HTMLButtonAttributes {
		selected?: boolean;
		children?: Snippet;
	}

	let {
		selected = false,
		disabled = false,
		children,
		onclick,
		...rest
	}: Props = $props();
</script>

<button
	type="button"
	class="select-chip"
	class:active={selected}
	{disabled}
	aria-pressed={selected}
	{onclick}
	{...rest}
>
	{@render children?.()}
</button>

<style>
	.select-chip {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.4rem 0.5rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		cursor: pointer;
		text-align: left;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
		word-break: break-all;
		transition:
			border-color 0.15s,
			background 0.15s;
	}

	.select-chip:hover:not(:disabled) {
		border-color: var(--black);
		background: var(--gray-100);
	}

	.select-chip.active {
		border-color: var(--black);
		color: var(--black);
		background: var(--gray-50);
	}

	.select-chip:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
