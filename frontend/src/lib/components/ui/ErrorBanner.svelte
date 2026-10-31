<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/ui/Icon.svelte';

	interface Props {
		message?: string;
		dismissable?: boolean;
		onDismiss?: () => void;
		children?: Snippet;
	}

	let {
		message = '',
		dismissable = false,
		onDismiss,
		children,
	}: Props = $props();

	let showClose = $derived(dismissable || onDismiss !== undefined);
</script>

<div class="error-banner">
	<div class="error-banner-content">
		{#if message}
			<span>{message}</span>
		{:else}
			{@render children?.()}
		{/if}
	</div>
	{#if showClose}
		<button
			type="button"
			class="error-banner-dismiss"
			onclick={() => onDismiss?.()}
			aria-label="Dismiss error"
		>
			<Icon name="close" />
		</button>
	{/if}
</div>

<style>
	.error-banner {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		background: var(--danger);
		color: var(--white);
		padding: 1rem;
		margin-bottom: 1.5rem;
		border: 1px solid var(--black);
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
	}

	.error-banner :global(button),
	.error-banner-dismiss {
		background: none;
		border: none;
		color: var(--white);
		cursor: pointer;
		font-size: 1.1rem;
		line-height: 1;
		flex-shrink: 0;
	}

	.error-banner-dismiss:hover {
		opacity: 0.8;
	}

	.error-banner-content {
		min-width: 0;
		flex: 1;
	}
</style>
