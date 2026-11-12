<script lang="ts">
	/**
	 * Metric card for overview KPI rows. Renders the global .kpi-card pattern
	 * inside a .kpi-grid parent. Pass a plain value, or children for custom
	 * content such as a Badge.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		value?: string;
		meta?: string;
		children?: Snippet;
	}

	let { label, value = '', meta = '', children }: Props = $props();
</script>

<div class="kpi-card">
	<span class="kpi-label">{label}</span>
	{#if children}
		{@render children()}
	{:else}
		<span class="kpi-value">{value}</span>
	{/if}
	{#if meta}
		<span class="kpi-meta">{meta}</span>
	{/if}
</div>
