<script lang="ts">
	/**
	 * Project workbench: project lifecycle on the left, index execution and
	 * maintenance for the selected project on the right. Indexing has exactly
	 * one entry point here (IndexControl), so the project list no longer
	 * duplicates the execution action.
	 */
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import FieldCheck from '$lib/components/ui/FieldCheck.svelte';
	import DescriptionList, {
		type DescriptionItem,
	} from '$lib/components/ui/DescriptionList.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import LazyPanel from '$lib/components/ui/LazyPanel.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { projectApi, type Project } from '$lib/api/index';
	import { toastStore } from '$lib/stores/toast.svelte';
	import { errorMessage, toAppError, type AppError } from '$lib/utils/errors';

	let loading = $state(false);
	let pageError = $state<AppError | null>(null);
	let formError = $state<string | null>(null);
	let deleteTarget = $state<Project | null>(null);

	// Create / edit form
	let showForm = $state(false);
	let editingProject = $state<Project | null>(null);
	let formName = $state('');
	let formPath = $state('');
	let formExtensions = $state('');
	let formExcludeDirs = $state('');
	let formRespectGitignore = $state(true);
	let formIgnorePatterns = $state('');

	let currentProject = $derived(projectStore.currentProject);

	function resetForm() {
		showForm = false;
		editingProject = null;
		formName = '';
		formPath = '';
		formExtensions = '';
		formExcludeDirs = '';
		formRespectGitignore = true;
		formIgnorePatterns = '';
		formError = null;
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
		formError = null;
		showForm = true;
	}

	async function handleSubmit() {
		if (!formName || !formPath) {
			formError = 'Project name and path are required';
			return;
		}

		loading = true;
		formError = null;

		try {
			const data = {
				name: formName,
				root_path: formPath,
				extensions: formExtensions
					? formExtensions
							.split(',')
							.map((s) => s.trim())
							.filter(Boolean)
					: undefined,
				exclude_dirs: formExcludeDirs
					? formExcludeDirs
							.split(',')
							.map((s) => s.trim())
							.filter(Boolean)
					: undefined,
				respect_gitignore: formRespectGitignore,
				ignore_patterns: formIgnorePatterns
					? formIgnorePatterns
							.split(',')
							.map((s) => s.trim())
							.filter(Boolean)
					: undefined,
			};

			if (editingProject) {
				await projectApi.updateProject(editingProject.id, data);
			} else {
				await projectApi.createProject(data);
			}

			resetForm();
			await projectStore.loadProjects();
		} catch (e) {
			formError =
				errorMessage(e) ||
				`Failed to ${editingProject ? 'update' : 'create'} project`;
		} finally {
			loading = false;
		}
	}

	function askDelete(project: Project) {
		deleteTarget = project;
	}

	async function confirmDelete() {
		const project = deleteTarget;
		if (!project) return;
		loading = true;
		pageError = null;

		try {
			await projectApi.deleteProject(project.id);
			deleteTarget = null;
			await projectStore.loadProjects();
		} catch (e) {
			pageError = toAppError(e, 'Failed to delete project');
		} finally {
			loading = false;
		}
	}

	async function handleReload(project: Project) {
		loading = true;
		pageError = null;

		try {
			const response = await projectApi.reloadProject(project.id);
			toastStore.success(response.message);
			await projectStore.loadProjects();
		} catch (e) {
			toastStore.error(errorMessage(e));
		} finally {
			loading = false;
		}
	}

	function selectProject(project: Project) {
		projectStore.currentProjectId = Number(project.id);
		resetForm();
	}

	let detailItems = $derived<DescriptionItem[]>(
		currentProject
			? [
					{ term: 'Name', value: currentProject.name },
					{ term: 'Path', value: currentProject.root_path, mono: true },
					...(currentProject.extensions?.length
						? [
								{
									term: 'Extensions',
									value: currentProject.extensions.join(', '),
								},
							]
						: []),
					...(currentProject.exclude_dirs?.length
						? [
								{
									term: 'Excluded Dirs',
									value: currentProject.exclude_dirs.join(', '),
								},
							]
						: []),
					...(currentProject.last_indexed
						? [{ term: 'Last Indexed', value: currentProject.last_indexed }]
						: []),
				]
			: [],
	);
</script>

<svelte:head>
	<title>Project Workbench - Code Context Engine</title>
</svelte:head>

<div class="page">
	<PageHeader
		title="Project Workbench"
		subtitle="Create and manage projects, then run indexing, parsing and index maintenance"
	/>

	{#if pageError}
		<ErrorBanner message={pageError.message} code={pageError.code} />
	{/if}

	<div class="workbench">
		<div class="list-column">
			<Card
				title="Projects"
				subtitle={`${projectStore.projects.length} project(s)`}
			>
				{#if showForm}
					<form
						onsubmit={(e) => {
							e.preventDefault();
							handleSubmit();
						}}
					>
						<div class="form-grid">
							<Input
								label="Project Name"
								type="text"
								bind:value={formName}
								required={true}
								placeholder="my-project"
							/>
							<Input
								label="Root Path"
								type="text"
								bind:value={formPath}
								required={true}
								placeholder="/path/to/project"
							/>
							<Input
								label="File Extensions (comma-separated)"
								type="text"
								bind:value={formExtensions}
								placeholder="rs, ts, js, py"
							/>
							<Input
								label="Exclude Directories (comma-separated)"
								type="text"
								bind:value={formExcludeDirs}
								placeholder="node_modules, target, .git"
							/>
							<Input
								label="Ignore Patterns (comma-separated)"
								type="text"
								bind:value={formIgnorePatterns}
								placeholder="*.log, *.tmp"
							/>
							<FieldCheck
								label="Respect .gitignore"
								bind:checked={formRespectGitignore}
							/>
						</div>

						{#if formError}
							<InlineError message={formError} />
						{/if}

						<div class="tool-actions">
							<Button type="submit" disabled={loading}>
								{editingProject ? 'Update' : 'Create'}
							</Button>
							<Button
								variant="secondary"
								onclick={resetForm}
								disabled={loading}
							>
								Cancel
							</Button>
						</div>
					</form>
				{:else}
					<div class="header-actions">
						<Button onclick={openCreateForm}>+ New Project</Button>
					</div>

					{#if projectStore.projects.length === 0}
						<EmptyState
							padded
							message="No projects found. Create your first project to get started."
						/>
					{:else}
						<div class="project-list">
							{#each projectStore.projects as project (project.id)}
								<button
									type="button"
									class="selectable-row"
									class:selected={currentProject?.id === project.id}
									aria-pressed={currentProject?.id === project.id}
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
				{/if}
			</Card>

			{#if currentProject && !showForm}
				<Card title={currentProject.name} subtitle="Project lifecycle actions">
					<DescriptionList items={detailItems} label="Project details" />

					<div class="tool-form-row">
						<Button
							variant="secondary"
							onclick={() => handleReload(currentProject!)}
							disabled={loading}
						>
							Reload Config
						</Button>
						<Button
							variant="secondary"
							onclick={() => openEditForm(currentProject!)}
							disabled={loading}
						>
							Edit
						</Button>
						<Button
							variant="danger"
							onclick={() => askDelete(currentProject!)}
							disabled={loading}
						>
							Delete
						</Button>
					</div>
				</Card>
			{/if}
		</div>

		<div class="index-column">
			{#if currentProject}
				<LazyPanel
					loader={() => import('$lib/components/index/IndexControl.svelte')}
					loadingMessage="Loading index control..."
				/>
				<LazyPanel
					loader={() => import('$lib/components/index/FileParser.svelte')}
					loadingMessage="Loading file parser..."
				/>
				<LazyPanel
					loader={() => import('$lib/components/index/IndexMaintenance.svelte')}
					loadingMessage="Loading index maintenance..."
				/>
			{:else}
				<Card
					title="Select a Project"
					subtitle="Choose a project from the list to manage its index"
				>
					<p class="empty-hint">
						Select a project on the left to control indexing, parse files and
						maintain its index data.
					</p>
				</Card>
			{/if}
		</div>
	</div>
</div>

{#if deleteTarget}
	<Modal title="Delete project?" onClose={() => (deleteTarget = null)}>
		<p>
			Delete project <strong>{deleteTarget.name}</strong> and forget its index configuration?
			This cannot be undone.
		</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (deleteTarget = null)}>
				Cancel
			</Button>
			<Button variant="danger" onclick={confirmDelete} disabled={loading}>
				{loading ? 'Deleting...' : 'Delete Project'}
			</Button>
		{/snippet}
	</Modal>
{/if}

<style>
	.workbench {
		display: grid;
		grid-template-columns: minmax(280px, 1fr) minmax(0, 2fr);
		gap: 2rem;
		align-items: start;
	}

	.list-column,
	.index-column {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		min-width: 0;
	}

	.header-actions {
		margin-bottom: 1.5rem;
	}

	.project-list {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.project-info {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-width: 0;
	}

	.project-info strong {
		font-size: 1rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.project-path {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--gray-500);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.project-indexed {
		font-family: var(--font-mono);
		font-size: 0.65rem;
		color: var(--gray-400);
		flex-shrink: 0;
	}

	@media (max-width: 1024px) {
		.workbench {
			grid-template-columns: 1fr;
		}
	}
</style>
