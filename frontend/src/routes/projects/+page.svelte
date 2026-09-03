<script lang="ts">
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { projects, selectedProject, loadProjects } from '$lib/stores/index';
	import { projectApi, type Project } from '$lib/api/index';
	import { currentProjectId } from '$lib/stores/project';

	let loading = $state(false);
	let error = $state<string | null>(null);

	// Create / Edit form
	let showForm = $state(false);
	let editingProject = $state<Project | null>(null);
	let formName = $state('');
	let formPath = $state('');
	let formExtensions = $state('');
	let formExcludeDirs = $state('');
	let formRespectGitignore = $state(true);
	let formIgnorePatterns = $state('');

	// Detail view
	let detailProject = $state<Project | null>(null);

	onMount(() => {
		loadProjects();
	});

	function resetForm() {
		showForm = false;
		editingProject = null;
		formName = '';
		formPath = '';
		formExtensions = '';
		formExcludeDirs = '';
		formRespectGitignore = true;
		formIgnorePatterns = '';
		error = null;
	}

	function openCreateForm() {
		resetForm();
		showForm = true;
	}

	function openEditForm(project: Project) {
		editingProject = project;
		formName = project.name;
		formPath = project.root_path;
		formExtensions = project.extensions?.join(', ') || '';
		formExcludeDirs = project.exclude_dirs?.join(', ') || '';
		formRespectGitignore = project.respect_gitignore ?? true;
		formIgnorePatterns = project.ignore_patterns?.join(', ') || '';
		showForm = true;
	}

	async function handleSubmit() {
		if (!formName || !formPath) {
			error = 'Project name and path are required';
			return;
		}

		loading = true;
		error = null;

		try {
			const data = {
				name: formName,
				root_path: formPath,
				extensions: formExtensions ? formExtensions.split(',').map(s => s.trim()).filter(Boolean) : undefined,
				exclude_dirs: formExcludeDirs ? formExcludeDirs.split(',').map(s => s.trim()).filter(Boolean) : undefined,
				respect_gitignore: formRespectGitignore,
				ignore_patterns: formIgnorePatterns ? formIgnorePatterns.split(',').map(s => s.trim()).filter(Boolean) : undefined,
			};

			if (editingProject) {
				await projectApi.updateProject(editingProject.id, data);
			} else {
				await projectApi.createProject(data);
			}

			resetForm();
			await loadProjects();
		} catch (e: any) {
			error = e.message || `Failed to ${editingProject ? 'update' : 'create'} project`;
		} finally {
			loading = false;
		}
	}

	async function handleDelete(project: Project) {
		if (!confirm(`Delete project "${project.name}"? This cannot be undone.`)) {
			return;
		}

		loading = true;
		error = null;

		try {
			await projectApi.deleteProject(project.id);
			if (detailProject?.id === project.id) {
				detailProject = null;
			}
			await loadProjects();
		} catch (e: any) {
			error = e.message || 'Failed to delete project';
		} finally {
			loading = false;
		}
	}

	async function handleIndex(project: Project) {
		loading = true;
		error = null;

		try {
			await projectApi.indexProject(project.id);
		} catch (e: any) {
			error = e.message || 'Failed to start indexing';
		} finally {
			loading = false;
		}
	}

	async function handleReload(project: Project) {
		loading = true;
		error = null;

		try {
			await projectApi.reloadProject(project.id);
		} catch (e: any) {
			error = e.message || 'Failed to reload project';
		} finally {
			loading = false;
		}
	}

	function selectProject(project: Project) {
		selectedProject.set(project);
		detailProject = project;
		currentProjectId.set(Number(project.id));
	}
</script>

<svelte:head>
	<title>Projects - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Projects</h1>
		<p class="page-description">Create, manage, and index your code projects</p>

		{#if error}
			<div class="error-banner">
				<span>{error}</span>
				<button class="dismiss-btn" onclick={() => error = null}>×</button>
			</div>
		{/if}

		<div class="layout">
			<!-- Project List -->
			<div class="list-column">
				<Card title="All Projects" subtitle={`${$projects.length} project(s)`}>
					<div class="header-actions">
						<Button onclick={openCreateForm} disabled={showForm}>
							+ New Project
						</Button>
					</div>

					{#if $projects.length === 0}
						<p class="empty-state">No projects found. Create your first project to get started.</p>
					{:else}
						<div class="project-list">
							{#each $projects as project (project.id)}
								<button
									class="project-item"
									class:selected={detailProject?.id === project.id}
									onclick={() => selectProject(project)}
								>
									<div class="project-info">
										<strong>{project.name}</strong>
										<span class="project-path">{project.root_path}</span>
									</div>
									{#if project.last_indexed}
										<span class="project-indexed">{project.last_indexed}</span>
									{/if}
								</button>
							{/each}
						</div>
					{/if}
				</Card>
			</div>

			<!-- Detail / Form Column -->
			<div class="detail-column">
				{#if showForm}
					<Card title={editingProject ? 'Edit Project' : 'Create Project'} subtitle="Configure project settings">
						<form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
							<div class="form-grid">
								<Input label="Project Name" type="text" bind:value={formName} required={true} placeholder="my-project" />
								<Input label="Root Path" type="text" bind:value={formPath} required={true} placeholder="/path/to/project" />
								<Input label="File Extensions (comma-separated)" type="text" bind:value={formExtensions} placeholder="rs, ts, js, py" />
								<Input label="Exclude Directories (comma-separated)" type="text" bind:value={formExcludeDirs} placeholder="node_modules, target, .git" />
								<Input label="Ignore Patterns (comma-separated)" type="text" bind:value={formIgnorePatterns} placeholder="*.log, *.tmp" />
								<div class="checkbox-field">
									<label class="checkbox-label">
										<input type="checkbox" bind:checked={formRespectGitignore} />
										Respect .gitignore
									</label>
								</div>
							</div>

							<div class="form-actions">
								<Button type="submit" disabled={loading}>
									{editingProject ? 'Update' : 'Create'}
								</Button>
								<Button variant="secondary" onclick={resetForm} disabled={loading}>
									Cancel
								</Button>
							</div>
						</form>
					</Card>

				{:else if detailProject}
					<Card title={detailProject.name} subtitle="Project details and actions">
						<div class="detail-grid">
							<div class="detail-item">
								<span class="detail-label">Name</span>
								<span class="detail-value">{detailProject.name}</span>
							</div>
							<div class="detail-item">
								<span class="detail-label">Path</span>
								<span class="detail-value code">{detailProject.root_path}</span>
							</div>
							{#if detailProject.extensions && detailProject.extensions.length > 0}
								<div class="detail-item">
									<span class="detail-label">Extensions</span>
									<span class="detail-value">{detailProject.extensions.join(', ')}</span>
								</div>
							{/if}
							{#if detailProject.exclude_dirs && detailProject.exclude_dirs.length > 0}
								<div class="detail-item">
									<span class="detail-label">Excluded Dirs</span>
									<span class="detail-value">{detailProject.exclude_dirs.join(', ')}</span>
								</div>
							{/if}
							{#if detailProject.last_indexed}
								<div class="detail-item">
									<span class="detail-label">Last Indexed</span>
									<span class="detail-value">{detailProject.last_indexed}</span>
								</div>
							{/if}
						</div>

						<h3 class="section-title">Actions</h3>
						<div class="action-buttons">
							<Button onclick={() => handleIndex(detailProject!)} disabled={loading}>
								Run Index
							</Button>
							<Button variant="secondary" onclick={() => handleReload(detailProject!)} disabled={loading}>
								Reload Config
							</Button>
							<Button variant="secondary" onclick={() => openEditForm(detailProject!)} disabled={loading}>
								Edit
							</Button>
							<Button variant="danger" onclick={() => handleDelete(detailProject!)} disabled={loading}>
								Delete
							</Button>
						</div>
					</Card>

				{:else}
					<Card title="Select a Project" subtitle="Choose a project from the list to view details">
						<p class="placeholder-text">Select a project on the left to view its details and available actions.</p>
					</Card>
				{/if}
			</div>
		</div>
	</div>
</section>

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 2rem;
	}

	.error-banner {
		background: var(--danger);
		color: var(--white);
		padding: 1rem;
		margin-bottom: 2rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		border: 1px solid var(--black);
	}

	.dismiss-btn {
		background: none;
		border: none;
		color: var(--white);
		font-size: 1.5rem;
		cursor: pointer;
		line-height: 1;
	}

	.layout {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2rem;
		align-items: start;
	}

	@media (max-width: 1024px) {
		.layout {
			grid-template-columns: 1fr;
		}
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
		gap: 0.5rem;
	}

	.project-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		width: 100%;
		padding: 1rem;
		border: 1px solid var(--gray-200);
		background: none;
		cursor: pointer;
		text-align: left;
		transition: all 0.2s;
		font-family: inherit;
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

	.project-info {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-width: 0;
	}

	.project-info strong {
		font-size: 0.95rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.project-path {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-500);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.project-indexed {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		color: var(--gray-400);
		flex-shrink: 0;
	}

	/* Detail / Form Column */
	.form-grid {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.checkbox-field {
		margin-top: 0.5rem;
	}

	.checkbox-label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.form-actions {
		display: flex;
		gap: 1rem;
		margin-top: 1.5rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.detail-grid {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}

	.detail-item {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--gray-100);
	}

	.detail-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.detail-value {
		font-size: 0.95rem;
	}

	.detail-value.code {
		font-family: 'Space Mono', monospace;
		font-size: 0.8rem;
		word-break: break-all;
	}

	.section-title {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1rem;
		font-weight: 700;
		margin-bottom: 0.75rem;
		letter-spacing: -0.03em;
	}

	.action-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.result-banner {
		margin-top: 1rem;
		padding: 0.75rem 1rem;
		background: var(--success-bg, #e6f7e6);
		border: 1px solid var(--success, #00a854);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
	}

	.placeholder-text {
		color: var(--gray-400);
		font-style: italic;
		text-align: center;
		padding: 3rem;
	}
</style>