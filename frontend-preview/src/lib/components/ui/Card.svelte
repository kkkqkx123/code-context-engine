<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		subtitle?: string;
		clickable?: boolean;
		children?: Snippet;
		[key: string]: unknown;
	}

	let {
		title = '',
		subtitle = '',
		clickable = false,
		children,
		...rest
	}: Props = $props();
</script>

<div
	class="card"
	class:card-clickable={clickable}
	{...rest}
>
	{#if title || subtitle}
		<div class="card-header">
			{#if title}
				<h3 class="card-title">{title}</h3>
			{/if}
			{#if subtitle}
				<p class="card-subtitle">{subtitle}</p>
			{/if}
		</div>
	{/if}
	<div class="card-content">
		{@render children?.()}
	</div>
</div>

<style>
	.card {
		background: var(--white);
		border: 1px solid var(--black);
		padding: 2rem;
		transition: background 0.3s;
	}

	.card-clickable {
		cursor: pointer;
	}

	.card-clickable:hover {
		background: var(--gray-100);
	}

	.card-header {
		margin-bottom: 1.5rem;
		padding-bottom: 1rem;
		border-bottom: 1px solid var(--gray-200);
	}

	.card-title {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.25rem;
		font-weight: 600;
		letter-spacing: -0.02em;
		margin-bottom: 0.5rem;
	}

	.card-subtitle {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}
</style>
