<script lang="ts">
	import { onMount } from 'svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		leftWidth?: number;
		minLeftWidth?: number;
		minRightWidth?: number;
		left?: Snippet;
		right?: Snippet;
	}

	let {
		leftWidth = 50,
		minLeftWidth = 20,
		minRightWidth = 20,
		left,
		right
	}: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let isDragging = $state(false);

	function startDrag() {
		isDragging = true;
		document.body.style.cursor = 'col-resize';
		document.body.style.userSelect = 'none';
	}

	function onDrag(e: MouseEvent) {
		if (!isDragging || !container) return;

		const rect = container.getBoundingClientRect();
		const newLeftWidth = ((e.clientX - rect.left) / rect.width) * 100;

		if (newLeftWidth >= minLeftWidth && newLeftWidth <= (100 - minRightWidth)) {
			leftWidth = newLeftWidth;
		}
	}

	function stopDrag() {
		isDragging = false;
		document.body.style.cursor = '';
		document.body.style.userSelect = '';
	}

	onMount(() => {
		window.addEventListener('mousemove', onDrag);
		window.addEventListener('mouseup', stopDrag);

		return () => {
			window.removeEventListener('mousemove', onDrag);
			window.removeEventListener('mouseup', stopDrag);
		};
	});
</script>

<div class="split-pane-container" bind:this={container}>
	<div class="pane pane-left" style="width: {leftWidth}%;">
		{@render left?.()}
	</div>

	<div
		class="divider"
		class:dragging={isDragging}
		onmousedown={startDrag}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				startDrag();
			}
		}}
		role="button"
		aria-label="Resize panels"
		tabindex="0"
	>
		<div class="divider-handle"></div>
	</div>

	<div class="pane pane-right" style="width: {100 - leftWidth}%;">
		{@render right?.()}
	</div>
</div>

<style>
	.split-pane-container {
		display: flex;
		width: 100%;
		height: 100%;
		position: relative;
	}

	.pane {
		height: 100%;
		overflow: auto;
	}

	.divider {
		width: 4px;
		background: var(--gray-200);
		cursor: col-resize;
		position: relative;
		transition: background 0.2s ease;
		flex-shrink: 0;
	}

	.divider:hover,
	.divider.dragging {
		background: var(--accent);
	}

	.divider-handle {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: 2px;
		height: 40px;
		background: var(--gray-500);
		pointer-events: none;
	}

	.divider:hover .divider-handle,
	.divider.dragging .divider-handle {
		background: var(--white);
	}
</style>
