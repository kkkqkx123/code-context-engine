<script lang="ts">
	/**
	 * Connectivity indicator dot. Renders the global .status-dot pattern;
	 * the state vocabulary (ok / bad / idle) is intentionally separate from
	 * the semantic Tone scale used by Badge and Notice.
	 */
	type StatusDotState = 'ok' | 'bad' | 'idle';

	interface Props {
		state?: StatusDotState;
		title?: string;
	}

	let { state = 'idle', title = '' }: Props = $props();
</script>

<span
	class="status-dot"
	class:ok={state === 'ok'}
	class:bad={state === 'bad'}
	class:idle={state === 'idle'}
	{title}
	role="img"
	aria-label={title || state}
></span>
