<script lang="ts">
	interface TabItem {
		id: string;
		label: string;
	}

	interface Props {
		tabs: TabItem[];
		active: string;
		onSelect: (id: string) => void;
		variant?: 'underline' | 'segmented';
		label?: string;
	}

	let {
		tabs,
		active,
		onSelect,
		variant = 'underline',
		label = 'Tabs',
	}: Props = $props();
</script>

<div
	class={variant === 'underline' ? 'tab-nav' : 'tabs-segmented'}
	role="tablist"
	aria-label={label}
>
	{#each tabs as tab (tab.id)}
		<button
			type="button"
			role="tab"
			aria-selected={active === tab.id}
			class={variant === 'underline' ? 'tab-btn' : ['btn', 'btn-tab']}
			class:active={active === tab.id}
			onclick={() => onSelect(tab.id)}
		>
			{tab.label}
		</button>
	{/each}
</div>

<style>
	.tabs-segmented {
		display: flex;
		gap: 0;
		margin-bottom: 1rem;
		border: 1px solid var(--black);
		background: var(--white);
	}
</style>
