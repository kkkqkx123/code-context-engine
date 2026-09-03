<script lang="ts">
	interface Props {
		progress?: number;
		showLabel?: boolean;
		label?: string;
		variant?: 'default' | 'success' | 'warning' | 'danger' | 'info';
	}

	let {
		progress = 0,
		showLabel = true,
		label = '',
		variant = 'default'
	}: Props = $props();

	let clampedProgress = $derived(Math.max(0, Math.min(100, progress)));
</script>

<div class="progress-container">
	<div class="progress-bar">
		<div
			class="progress-fill"
			class:fill-success={variant === 'success'}
			class:fill-warning={variant === 'warning'}
			class:fill-danger={variant === 'danger'}
			class:fill-info={variant === 'info'}
			style="width: {clampedProgress}%"
		></div>
	</div>
	{#if showLabel}
		<div class="progress-label">
			{#if label}
				{label}
			{:else}
				{Math.round(clampedProgress)}%
			{/if}
		</div>
	{/if}
</div>

<style>
	.progress-container {
		width: 100%;
	}

	.progress-bar {
		width: 100%;
		height: 4px;
		background: var(--gray-200);
		position: relative;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--black);
		transition: width 0.3s ease;
	}

	.fill-success {
		background: var(--success);
	}

	.fill-warning {
		background: var(--warning);
	}

	.fill-danger {
		background: var(--danger);
	}

	.fill-info {
		background: var(--info);
	}

	.progress-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-top: 0.5rem;
	}
</style>
