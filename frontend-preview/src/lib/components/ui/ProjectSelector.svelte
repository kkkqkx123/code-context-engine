<script lang="ts">
	/**
	 * Selector for the globally selected project.
	 * Renders in the sidebar on desktop and in the topbar on narrow viewports,
	 * where the sidebar collapses into a drawer.
	 */
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		projects,
		currentProject,
		currentProjectId,
	} from '$lib/stores/project';

	let { variant = 'sidebar' }: { variant?: 'sidebar' | 'topbar' } = $props();

	let selectId = $derived(
		variant === 'sidebar' ? 'project-select' : 'project-select-topbar',
	);

	function handleChange(event: Event) {
		const target = event.currentTarget as HTMLSelectElement;
		currentProjectId.set(Number(target.value));
	}

	function goToProjects(event: MouseEvent) {
		event.preventDefault();
		goto(resolve('/projects'));
	}
</script>

<div class="project-selector" class:topbar={variant === 'topbar'}>
	{#if variant === 'sidebar'}
		<label class="project-label" for={selectId}>Current Project</label>
	{/if}

	{#if $projects.length === 0}
		<a class="empty-link" href={resolve('/projects')} onclick={goToProjects}>
			Create a project
		</a>
	{:else}
		<select
			id={selectId}
			class="project-select"
			aria-label="Current Project"
			title={$currentProject?.root_path ?? ''}
			onchange={handleChange}
		>
			{#each $projects as project (project.id)}
				<option
					value={Number(project.id)}
					selected={Number(project.id) === $currentProjectId}
				>
					{project.name || `#${project.id}`}
				</option>
			{/each}
		</select>
		{#if variant === 'sidebar' && $currentProject}
			<p class="project-path" title={$currentProject.root_path}>
				{$currentProject.root_path}
			</p>
		{/if}
	{/if}
</div>

<style>
	.project-selector {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-width: 0;
	}

	.project-selector.topbar {
		flex-direction: row;
		align-items: center;
	}

	.project-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-500);
	}

	.project-select {
		width: 100%;
		padding: 0.5rem;
		background: var(--gray-900);
		color: var(--white);
		border: 1px solid var(--gray-700);
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}

	.project-select:focus {
		outline: none;
		border-color: var(--accent);
	}

	.topbar .project-select {
		background: var(--white);
		color: var(--black);
		border: 1px solid var(--black);
		max-width: 11rem;
	}

	.project-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--gray-500);
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.empty-link {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--accent);
		text-decoration: none;
		border: 1px dashed var(--gray-700);
		padding: 0.5rem;
		text-align: center;
	}

	.empty-link:hover {
		border-color: var(--accent);
	}
</style>
