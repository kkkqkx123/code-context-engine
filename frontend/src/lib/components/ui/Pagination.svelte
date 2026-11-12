<script lang="ts">
	/**
	 * Previous/next page row. Renders the global .pagination pattern.
	 * A null total means the backend cannot report the page count, in which
	 * case only the current page is shown.
	 */
	import Button from './Button.svelte';

	interface Props {
		page: number;
		total?: number | string | null;
		onPrev: () => void;
		onNext: () => void;
		prevDisabled?: boolean;
		nextDisabled?: boolean;
		prevLabel?: string;
		nextLabel?: string;
		size?: 'xs' | 'sm' | 'md' | 'lg';
		label?: string;
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
	}: Props = $props();

	let info = $derived(
		total === null ? `Page ${page}` : `Page ${page} of ${total}`,
	);
</script>

<div class="pagination" role="navigation" aria-label={label}>
	<Button variant="secondary" {size} onclick={onPrev} disabled={prevDisabled}>
		{prevLabel}
	</Button>
	<span class="page-info">{info}</span>
	<Button variant="secondary" {size} onclick={onNext} disabled={nextDisabled}>
		{nextLabel}
	</Button>
</div>
