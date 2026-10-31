<script lang="ts">
	/**
	 * Selector for the globally selected project.
	 * Styling is driven by --chrome-* tokens set by the surrounding
	 * layout context (sidebar drawer vs topbar), so the component itself
	 * carries a single visual definition.
	 */
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { projectStore } from '$lib/stores/project.svelte';

	let selectId = 'project-select';

	function handleChange(event: Event) {
		const target = event.currentTarget as HTMLSelectElement;
		projectStore.currentProjectId = Number(target.value);
	}

	function goToProjects(event: MouseEvent) {
		event.preventDefault();
		goto(resolve('/projects'));
	}
</script>

<div class="project-selector">
	<label class="project-label" for={selectId}>Current Project</label>

	{#if projectStore.projects.length === 0}
		<a class="empty-link" href={resolve('/projects')} onclick={goToProjects}>
			Create a project
		</a>
	{:else}
		<select
			id={selectId}
			class="project-select"
			aria-label="Current Project"
			title={projectStore.currentProject?.root_path ?? ''}
			onchange={handleChange}
		>
			{#each projectStore.projects as project (project.id)}
				<option
					value={Number(project.id)}
					selected={Number(project.id) === projectStore.currentProjectId}
				>
					{project.name || `#${project.id}`}
				</option>
			{/each}
		</select>
		{#if projectStore.currentProject}
			<p class="project-path" title={projectStore.currentProject.root_path}>
				{projectStore.currentProject.root_path}
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

	.project-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--chrome-muted, var(--gray-500));
	}

	.project-select {
		width: 100%;
		padding: 0.5rem;
		background: var(--chrome-input-bg, var(--gray-900));
		color: var(--chrome-input-fg, var(--white));
		border: 1px solid var(--chrome-input-border, var(--gray-700));
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
	}

	.project-select:focus {
		outline: none;
		border-color: var(--accent);
	}

	.project-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--chrome-muted, var(--gray-500));
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
		border: 1px dashed var(--chrome-input-border, var(--gray-700));
		padding: 0.5rem;
		text-align: center;
	}

	.empty-link:hover {
		border-color: var(--accent);
	}
</style>
