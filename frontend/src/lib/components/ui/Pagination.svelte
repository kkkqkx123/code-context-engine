<script lang="ts">
	/**
	 * Previous/next page row. Renders the global .pagination pattern.
	 * A null total means the page count is unknown (cumulative fetch), in
	 * which case only the current page is shown.
	 */
	import Button from './Button.svelte';

	interface Props {
		page: number;
		total?: number | null;
		onPrev: () => void;
		onNext: () => void;
		prevDisabled?: boolean;
		nextDisabled?: boolean;
		prevLabel?: string;
		nextLabel?: string;
		size?: 'xs' | 'sm' | 'md' | 'lg';
		label?: string;
		/** Optional note clarifying what Next does (paging vs. cumulative load). */
		hint?: string;
	}

	let {
		page,
		total = null,
		onPrev,
		onNext,
		prevDisabled = false,
		nextDisabled = false,
		prevLabel = 'Previous',
		nextLabel = 'Next',
		size = 'md',
		label = 'Pagination',
		hint = '',
	}: Props = $props();

	let info = $derived(
		total === null ? `Page ${page}` : `Page ${page} of ${total}`,
	);
</script>

<div class="pagination-group">
	<div class="pagination" role="navigation" aria-label={label}>
		<Button
			variant="secondary"
			{size}
			onclick={onPrev}
			disabled={prevDisabled}
		>
			{prevLabel}
		</Button>
		<span class="page-info">{info}</span>
		<Button
			variant="secondary"
			{size}
			onclick={onNext}
			disabled={nextDisabled}
		>
			{nextLabel}
		</Button>
	</div>
	{#if hint}
		<p class="pagination-hint">{hint}</p>
	{/if}
</div>

<style>
	.pagination-hint {
		margin: 0.5rem 0 0;
		text-align: center;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
	}
</style>
