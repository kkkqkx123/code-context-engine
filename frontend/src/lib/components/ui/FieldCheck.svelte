<script lang="ts">
	/**
	 * Inline checkbox row. Renders the global .check-row pattern with a
	 * native input so keyboard and screen-reader behavior stay default.
	 * The Toggle component remains the choice for switch-style settings.
	 */
	interface Props {
		label: string;
		checked?: boolean;
		disabled?: boolean;
	}

	let { label, checked = $bindable(false), disabled = false }: Props = $props();
</script>

<label class="check-row">
	<input type="checkbox" bind:checked {disabled} />
	<span>{label}</span>
</label>
