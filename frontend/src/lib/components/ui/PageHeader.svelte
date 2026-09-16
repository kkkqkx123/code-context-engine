<script lang="ts">
	/**
	 * Consistent page header: title + one-line description on the left,
	 * optional action slot on the right. Replaces the legacy giant hero.
	 */
	let {
		title,
		subtitle = '',
		children,
	}: {
		title: string;
		subtitle?: string;
		children?: any;
	} = $props();
</script>

<header class="page-header">
	<div class="page-header-text">
		<h1>{title}</h1>
		{#if subtitle}
			<p class="page-header-subtitle">{subtitle}</p>
		{/if}
	</div>
	{#if children}
		<div class="page-header-actions">
			{@render children()}
		</div>
	{/if}
</header>

<style>
	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: flex-end;
		gap: 1.5rem;
		padding-bottom: 1.25rem;
		margin-bottom: 1.75rem;
		border-bottom: 1px solid var(--black);
	}

	.page-header-text {
		min-width: 0;
	}

	.page-header h1 {
		font-size: clamp(1.5rem, 3vw, 2rem);
		line-height: 1.1;
		letter-spacing: -0.03em;
		margin: 0;
	}

	.page-header-subtitle {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 0.95rem;
		color: var(--gray-600);
		margin: 0.5rem 0 0;
	}

	.page-header-actions {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-shrink: 0;
	}

	@media (max-width: 768px) {
		.page-header {
			flex-direction: column;
			align-items: stretch;
			gap: 1rem;
		}

		.page-header-actions {
			flex-wrap: wrap;
		}
	}
</style>
