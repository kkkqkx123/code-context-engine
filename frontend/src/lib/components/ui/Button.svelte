<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import type { Snippet } from 'svelte';

	interface Props extends HTMLButtonAttributes {
		variant?: 'primary' | 'secondary' | 'danger' | 'ghost' | 'tab';
		size?: 'xs' | 'sm' | 'md' | 'lg';
		block?: boolean;
		loading?: boolean;
		active?: boolean;
		children?: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		block = false,
		loading = false,
		active = false,
		type = 'button',
		disabled = false,
		class: className = undefined,
		children,
		...rest
	}: Props = $props();
</script>

<button
	{type}
	class={['btn', className]}
	class:btn-secondary={variant === 'secondary'}
	class:btn-danger={variant === 'danger'}
	class:btn-ghost={variant === 'ghost'}
	class:btn-tab={variant === 'tab'}
	class:btn-xs={size === 'xs'}
	class:btn-sm={size === 'sm'}
	class:btn-lg={size === 'lg'}
	class:btn-block={block}
	class:active
	{disabled}
	{...rest}
>
	{#if loading}
		<span class="loading-indicator">...</span>
	{/if}
	{@render children?.()}
</button>

<style>
	.loading-indicator {
		animation: pulse 1s infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}
</style>
