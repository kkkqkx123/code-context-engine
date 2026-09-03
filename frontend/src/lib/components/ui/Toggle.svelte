<script lang="ts">
	interface Props {
		checked?: boolean;
		label?: string;
		disabled?: boolean;
		onchange?: (e: { checked: boolean }) => void;
	}

	let {
		checked = false,
		label = '',
		disabled = false,
		onchange
	}: Props = $props();

	function toggle() {
		if (disabled) return;
		checked = !checked;
		onchange?.({ checked });
	}
</script>

<button
	type="button"
	class="toggle-switch"
	class:active={checked}
	class:disabled
	onclick={toggle}
	aria-checked={checked}
	role="switch"
>
	<span class="toggle-track">
		<span class="toggle-thumb"></span>
	</span>
	{#if label}
		<span class="toggle-label">{label}</span>
	{/if}
</button>

<style>
	.toggle-switch {
		display: inline-flex;
		align-items: center;
		gap: 0.75rem;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: inherit;
	}

	.toggle-switch.disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.toggle-track {
		position: relative;
		width: 48px;
		height: 24px;
		background: var(--gray-200);
		border: 1px solid var(--black);
		transition: background 0.3s ease;
	}

	.toggle-switch.active .toggle-track {
		background: var(--accent);
	}

	.toggle-thumb {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 18px;
		height: 18px;
		background: var(--white);
		border: 1px solid var(--black);
		transition: transform 0.3s cubic-bezier(0.2, 0.9, 0.2, 1);
	}

	.toggle-switch.active .toggle-thumb {
		transform: translateX(24px);
	}

	.toggle-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		user-select: none;
	}
</style>
