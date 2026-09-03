<script lang="ts">
	import { onMount } from 'svelte';
	import { projects, selectedProject, loadProjects } from '$lib/stores/index';
	import { projectApi, type Project } from '$lib/api/index';
	import Card from '../ui/Card.svelte';
	import Button from '../ui/Button.svelte';
	import Input from '../ui/Input.svelte';
	import Badge from '../ui/Badge.svelte';

	let showCreateForm = $state(false);
	let showEditForm = $state(false);
	let editingProject = $state<Project | null>(null);

	// Form state
	let projectName = $state('');
	let projectPath = $state('');
	let extensions = $state('');
	let excludePatterns = $state('');

	onMount(() => {
		loadProjects();
	});

	function resetForm() {
		projectName = '';
		projectPath = '';
		extensions = '';
		excludePatterns = '';
		showCreateForm = false;
		showEditForm = false;
		editingProject = null;
	}

	async function handleCreate() {
		if (!projectName || !projectPath) {
			alert('Project name and path are required');
			return;
		}

		try {
			await projectApi.createProject({
				name: projectName,
				root_path: projectPath,
				extensions: extensions ? extensions.split(',').map(e => e.trim()).filter(Boolean) : undefined,
				exclude_dirs: excludePatterns ? excludePatterns.split(',').map(e => e.trim()).filter(Boolean) : undefined,
			});
			resetForm();
			await loadProjects();
		} catch (error: any) {
			alert(`Failed to create project: ${error.message}`);
		}
	}

	function handleEdit(project: Project) {
		editingProject = project;
		projectName = project.name;
		projectPath = project.root_path;
		extensions = project.extensions?.join(', ') || '';
		excludePatterns = project.exclude_dirs?.join(', ') || '';
		showEditForm = true;
	}

	async function handleUpdate() {
		if (!editingProject || !projectName || !projectPath) {
			alert('Project name and path are required');
			return;
		}

		try {
			await projectApi.updateProject(editingProject.id, {
				name: projectName,
				extensions: extensions ? extensions.split(',').map(e => e.trim()).filter(Boolean) : undefined,
				exclude_dirs: excludePatterns ? excludePatterns.split(',').map(e => e.trim()).filter(Boolean) : undefined,
			});
			resetForm();
			await loadProjects();
		} catch (error: any) {
			alert(`Failed to update project: ${error.message}`);
		}
	}

	async function handleDelete(id: string) {
		if (!confirm('Are you sure you want to delete this project?')) {
			return;
		}

		try {
			await projectApi.deleteProject(id);
			await loadProjects();
		} catch (error: any) {
			alert(`Failed to delete project: ${error.message}`);
		}
	}

	async function handleIndex(id: string) {
		try {
			await projectApi.indexProject(id);
			alert('Indexing started successfully');
		} catch (error: any) {
			alert(`Failed to start indexing: ${error.message}`);
		}
	}

	function selectProject(project: Project) {
		selectedProject.set(project);
	}
</script>

<Card title="Projects" subtitle="Manage indexed projects">
	{#if showCreateForm || showEditForm}
		<div class="form-section">
			<h3>{showEditForm ? 'Edit Project' : 'Create New Project'}</h3>
			
			<form onsubmit={(e) => { e.preventDefault(); showEditForm ? handleUpdate() : handleCreate(); }}>
				<Input
					label="Project Name"
					type="text"
					bind:value={projectName}
					required={true}
					placeholder="my-project"
				/>

				<Input
					label="Project Path"
					type="text"
					bind:value={projectPath}
					required={true}
					placeholder="/path/to/project"
				/>

				<Input
					label="File Extensions (comma-separated)"
					type="text"
					bind:value={extensions}
					placeholder="rs, ts, js, py"
				/>

				<Input
					label="Exclude Patterns (comma-separated)"
					type="text"
					bind:value={excludePatterns}
					placeholder="node_modules, target, .git"
				/>

				<div class="form-actions">
					<Button type="submit" variant="primary">
						{showEditForm ? 'Update' : 'Create'}
					</Button>
					<Button type="button" variant="secondary" onclick={resetForm}>
						Cancel
					</Button>
				</div>
			</form>
		</div>
	{/if}

	{#if !showCreateForm && !showEditForm}
		<div class="header-actions">
			<Button variant="primary" onclick={() => showCreateForm = true}>
				+ New Project
			</Button>
		</div>
	{/if}

	{#if $projects.length === 0}
		<p class="empty-state">No projects found. Create your first project above.</p>
	{:else}
		<div class="project-list">
			{#each $projects as project (project.id)}
				<div 
					class="project-item" 
					class:selected={$selectedProject?.id === project.id}
					onclick={() => selectProject(project)}
					role="button"
					tabindex="0"
					onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') selectProject(project); }}
				>
					<div class="project-info">
						<div class="project-header">
							<h4>{project.name}</h4>
							<Badge variant="active">Active</Badge>
						</div>
						<p class="project-path">{project.root_path}</p>
						{#if project.extensions && project.extensions.length > 0}
							<div class="project-meta">
								<span class="meta-label">Extensions:</span>
								<span class="meta-value">{project.extensions.join(', ')}</span>
							</div>
						{/if}
						{#if project.exclude_dirs && project.exclude_dirs.length > 0}
							<div class="project-meta">
								<span class="meta-label">Excluded:</span>
								<span class="meta-value">{project.exclude_dirs.join(', ')}</span>
							</div>
						{/if}
					</div>
					<div class="project-actions">
						<Button variant="secondary" size="sm" onclick={() => handleIndex(project.id)}>
							Index
						</Button>
						<Button variant="secondary" size="sm" onclick={() => handleEdit(project)}>
							Edit
						</Button>
						<Button variant="danger" size="sm" onclick={() => handleDelete(project.id)}>
							Delete
						</Button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</Card>

<style>
	.form-section {
		margin-bottom: 2rem;
		padding: 1.5rem;
		border: 1px solid var(--black);
		background: var(--gray-100);
	}

	.form-section h3 {
		margin-bottom: 1.5rem;
		font-size: 1.25rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.form-actions {
		display: flex;
		gap: 1rem;
		margin-top: 1rem;
	}

	.header-actions {
		margin-bottom: 1.5rem;
	}

	.empty-state {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 3rem 0;
	}

	.project-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.project-item {
		padding: 1.5rem;
		border: 1px solid var(--gray-200);
		cursor: pointer;
		transition: all 0.2s;
	}

	.project-item:hover {
		background: var(--gray-100);
		border-color: var(--black);
	}

	.project-item.selected {
		border-color: var(--accent);
		border-left: 4px solid var(--accent);
		background: var(--gray-100);
	}

	.project-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.75rem;
	}

	.project-header h4 {
		font-size: 1.1rem;
		margin: 0;
	}

	.project-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.85rem;
		color: var(--gray-600);
		margin-bottom: 0.75rem;
	}

	.project-meta {
		display: flex;
		gap: 0.5rem;
		font-size: 0.85rem;
		margin-bottom: 0.5rem;
	}

	.meta-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.meta-value {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.project-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}
</style>
