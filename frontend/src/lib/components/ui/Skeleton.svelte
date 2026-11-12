<script lang="ts">
	/**
	 * Content placeholder rows. Renders the global .skeleton pattern while
	 * the real content loads. Keep the line count close to the expected
	 * content height to avoid layout shift.
	 */
	interface Props {
		lines?: number;
		label?: string;
	}

	let { lines = 3, label = 'Loading content' }: Props = $props();

	let rows = $derived(
		Array.from({ length: Math.max(1, lines) }, (_, i) => i),
	);
</script>

<div
	class="skeleton"
	role="status"
	aria-label={label}
	aria-busy="true"
>
	{#each rows as row (row)}
		<div
			class="skeleton-line"
			style={row === rows.length - 1 ? 'width: 55%' : 'width: 100%'}
		></div>
	{/each}
</div>
