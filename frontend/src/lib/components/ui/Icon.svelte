<script lang="ts">
	/**
	 * Lightweight icon component.
	 *
	 * The project intentionally avoids icon libraries (see
	 * docs/specs/frontend-functional-analysis.md). Icons are therefore
	 * rendered from a single, centrally maintained glyph map so that call
	 * sites stay consistent and accessible.
	 */
	type IconName =
		| 'success'
		| 'error'
		| 'warning'
		| 'info'
		| 'close'
		| 'arrow-right'
		| 'arrow-up'
		| 'arrow-down'
		| 'diamond'
		| 'check'
		| 'minus';

	interface Props {
		name: IconName;
		/** When provided, the icon is announced to assistive technology. */
		label?: string;
		class?: string;
	}

	let { name, label = '', class: className = '' }: Props = $props();

	const GLYPHS: Record<IconName, string> = {
		success: '✓',
		error: '✕',
		warning: '⚠',
		info: 'ℹ',
		close: '×',
		'arrow-right': '→',
		'arrow-up': '▲',
		'arrow-down': '▼',
		diamond: '◆',
		check: '✓',
		minus: '−',
	};
</script>

<span
	class={['icon', className]}
	aria-hidden={label ? undefined : 'true'}
	aria-label={label || undefined}
	role={label ? 'img' : undefined}
>
	{GLYPHS[name]}
</span>

<style>
	.icon {
		font-style: normal;
		line-height: 1;
		display: inline-block;
	}
</style>
