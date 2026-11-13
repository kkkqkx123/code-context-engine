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
		onchange,
	}: Props = $props();

	function toggle() {
		if (disabled) return;
		// The owner holds the state; the switch only reports the intent. Keeping
		// a local copy would desync whenever the owner ignores the callback.
		onchange?.({ checked: !checked });
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
		/* Every dimension the thumb travel depends on is declared once here, so
		 * the travel below is derived instead of restated as a pixel constant. */
		--toggle-track-width: 48px;
		--toggle-track-height: 24px;
		--toggle-thumb-size: 18px;
		--toggle-inset: 2px;
		--toggle-border-width: 1px;
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
		/* The thumb is positioned against this box and contributes nothing to its
		 * min-content size, so without `flex: none` a narrow container shrinks the
		 * track while the thumb keeps its full travel and sticks out of it. */
		flex: none;
		position: relative;
		width: var(--toggle-track-width);
		height: var(--toggle-track-height);
		background: var(--gray-200);
		border: var(--toggle-border-width) solid var(--black);
		transition: background 0.3s ease;
	}

	.toggle-switch.active .toggle-track {
		background: var(--accent);
	}

	.toggle-thumb {
		position: absolute;
		top: var(--toggle-inset);
		left: var(--toggle-inset);
		width: var(--toggle-thumb-size);
		height: var(--toggle-thumb-size);
		background: var(--white);
		border: var(--toggle-border-width) solid var(--black);
		transition: transform 0.3s cubic-bezier(0.2, 0.9, 0.2, 1);
	}

	.toggle-switch.active .toggle-thumb {
		/* Interior width minus the thumb and the inset it started from. */
		transform: translateX(
			calc(
				var(--toggle-track-width) - var(--toggle-thumb-size) - 2 *
					var(--toggle-inset) - 2 * var(--toggle-border-width)
			)
		);
	}

	.toggle-label {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: var(--tracking-widest);
		color: var(--gray-600);
		user-select: none;
	}
</style>
