<script lang="ts">
	import type { Snippet } from 'svelte';
	import { currentProject } from '$lib/stores/project';
	/**
	 * Consistent page header: title + one-line description on the left,
	 * optional action slot on the right. Replaces the legacy giant hero.
	 * The globally selected project is echoed underneath so every page states
	 * which project its data belongs to.
	 */
	let {
		title,
		subtitle = '',
		children,
	}: {
		title: string;
		subtitle?: string;
		children?: Snippet;
	} = $props();
</script>

<header class="page-header">
	<div class="page-header-text">
		<h1>{title}</h1>
		{#if subtitle}
			<p class="page-header-subtitle">{subtitle}</p>
		{/if}
		{#if $currentProject}
			<p class="page-header-project" title={$currentProject.root_path}>
				<span class="page-header-project-name">{$currentProject.name}</span>
				<span class="page-header-project-sep">·</span>
				<span class="page-header-project-path">{$currentProject.root_path}</span
				>
			</p>
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

	.page-header-project {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin: 0.35rem 0 0;
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		min-width: 0;
	}

	.page-header-project-name {
		color: var(--black);
		font-weight: 700;
		flex-shrink: 0;
	}

	.page-header-project-sep {
		color: var(--gray-400);
	}

	.page-header-project-path {
		color: var(--gray-500);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
