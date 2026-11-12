<script lang="ts">
	/**
	 * Term/value detail block. Renders the global .desc-list pattern with
	 * semantic dl/dt/dd markup. Prefer this over hand-written detail grids.
	 */
	export interface DescriptionItem {
		term: string;
		value: string;
		mono?: boolean;
	}

	interface Props {
		items: DescriptionItem[];
		label?: string;
	}

	let { items, label = 'Details' }: Props = $props();
</script>

<dl class="desc-list" aria-label={label}>
	{#each items as item (item.term)}
		<div class="desc-item">
			<dt class="desc-term">{item.term}</dt>
			<dd class="desc-value" class:desc-value-mono={item.mono}>
				{item.value}
			</dd>
		</div>
	{/each}
</dl>
