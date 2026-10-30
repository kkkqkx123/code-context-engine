<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import {
		indexApi,
		type DeleteFileResponse,
	} from '$lib/api/index';
	import { projectStore } from '$lib/stores/project.svelte';

	interface Props {
		/** Called after any successful deletion so stats can be refreshed. */
		onDeleted?: () => Promise<void> | void;
	}

	let { onDeleted }: Props = $props();

	// ─── Delete file ─────────────────────────────────────────────
	let deleteFilePath = $state('');
	let deleteFileArmed = $state(false);
	let deleteFileBusy = $state(false);
	let deleteFileResult = $state<DeleteFileResponse | null>(null);
	let deleteFileError = $state('');

	async function runDeleteFile() {
		const path = deleteFilePath.trim();
		if (!path) return;
		if (!deleteFileArmed) {
			deleteFileArmed = true;
			return;
		}
		deleteFileBusy = true;
		deleteFileError = '';
		try {
			deleteFileResult = await indexApi.deleteFile(path, projectStore.currentProjectId);
			deleteFileArmed = false;
			deleteFilePath = '';
			await onDeleted?.();
		} catch (e) {
			deleteFileError = errorMessage(e) ?? 'Failed to delete file';
		} finally {
			deleteFileBusy = false;
		}
	}

	// ─── Delete entity ───────────────────────────────────────────
	let deleteEntityId = $state('');
	let deleteEntityArmed = $state(false);
	let deleteEntityBusy = $state(false);
	let deleteEntityMessage = $state('');
	let deleteEntityError = $state('');

	async function runDeleteEntity() {
		const id = Number(deleteEntityId.trim());
		if (!Number.isFinite(id) || id <= 0) {
			deleteEntityError = 'Enter a numeric entity id';
			return;
		}
		if (!deleteEntityArmed) {
			deleteEntityArmed = true;
			return;
		}
		deleteEntityBusy = true;
		deleteEntityError = '';
		try {
			const response = await indexApi.deleteEntity(id, projectStore.currentProjectId);
			deleteEntityMessage = response.message || `Entity ${id} deleted`;
			deleteEntityArmed = false;
			deleteEntityId = '';
			await onDeleted?.();
		} catch (e) {
			deleteEntityError = errorMessage(e) ?? 'Failed to delete entity';
		} finally {
			deleteEntityBusy = false;
		}
	}

	// ─── Batch delete ────────────────────────────────────────────
	let batchFilesText = $state('');
	let batchEntitiesText = $state('');
	let batchArmed = $state(false);
	let batchBusy = $state(false);
	let batchResult = $state<{
		files_deleted: number;
		entities_deleted: number;
	} | null>(null);
	let batchError = $state('');

	async function runBatchDelete() {
		const filePaths = batchFilesText
			.split('\n')
			.map((l) => l.trim())
			.filter((l) => l.length > 0);
		const entityIds = batchEntitiesText
			.split(/[\s,]+/)
			.map((t) => Number(t.trim()))
			.filter((n) => Number.isFinite(n) && n > 0);
		if (filePaths.length === 0 && entityIds.length === 0) {
			batchError = 'Provide at least one file path or entity id';
			return;
		}
		if (!batchArmed) {
			batchArmed = true;
			return;
		}
		batchBusy = true;
		batchError = '';
		try {
			const response = await indexApi.batchDelete(projectStore.currentProjectId, {
				file_paths: filePaths.length > 0 ? filePaths : undefined,
				entity_ids: entityIds.length > 0 ? entityIds : undefined,
			});
			batchResult = {
				files_deleted: response.files_deleted,
				entities_deleted: response.entities_deleted,
			};
			batchArmed = false;
			batchFilesText = '';
			batchEntitiesText = '';
			await onDeleted?.();
		} catch (e) {
			batchError = errorMessage(e) ?? 'Batch delete failed';
		} finally {
			batchBusy = false;
		}
	}
</script>

<!--
	Destructive maintenance operations for the index page: single file
	delete, single entity delete and batch delete. Each action requires a
	second confirmation click before it runs.
-->
<Card
	title="Delete File"
	subtitle="Remove a file and its derived data from all backends"
>
	<div class="action-row">
		<input
			class="text-input text-input--inline"
			type="text"
			bind:value={deleteFilePath}
			placeholder="/path/to/file.ts"
			aria-label="File path to delete"
		/>
		<Button
			variant={deleteFileArmed ? 'danger' : 'secondary'}
			onclick={runDeleteFile}
			disabled={deleteFileBusy || !deleteFilePath.trim()}
		>
			{#if deleteFileBusy}Deleting...{:else if deleteFileArmed}Confirm Delete{:else}Delete
				File{/if}
		</Button>
	</div>
	{#if deleteFileArmed}
		<p class="arm-warning">
			Click again to permanently delete this file's index data.
		</p>
	{/if}
	{#if deleteFileError}
		<InlineError message={deleteFileError} />
	{/if}
	{#if deleteFileResult}
		<div class="result-row">
			<Badge
				label={deleteFileResult.success ? 'Deleted' : 'Failed'}
				variant={deleteFileResult.success ? 'success' : 'danger'}
			/>
			<span class="mono">
				{deleteFileResult.vectors_deleted} vectors · {deleteFileResult.bm25_documents_deleted}
				BM25 docs · {deleteFileResult.relations_deleted} relations · {deleteFileResult.elapsed_ms}
				ms
			</span>
		</div>
	{/if}
</Card>

<Card title="Delete Entity" subtitle="Remove a single entity by id">
	<div class="action-row">
		<input
			class="text-input text-input--inline"
			type="text"
			bind:value={deleteEntityId}
			placeholder="Entity id (numeric)"
			aria-label="Entity id to delete"
		/>
		<Button
			variant={deleteEntityArmed ? 'danger' : 'secondary'}
			onclick={runDeleteEntity}
			disabled={deleteEntityBusy || !deleteEntityId.trim()}
		>
			{#if deleteEntityBusy}Deleting...{:else if deleteEntityArmed}Confirm
				Delete{:else}Delete Entity{/if}
		</Button>
	</div>
	{#if deleteEntityArmed}
		<p class="arm-warning">Click again to permanently delete this entity.</p>
	{/if}
	{#if deleteEntityError}
		<InlineError message={deleteEntityError} />
	{/if}
	{#if deleteEntityMessage}
		<div class="result-row">
			<Badge label="Deleted" variant="success" />
			<span class="mono">{deleteEntityMessage}</span>
		</div>
	{/if}
</Card>

<Card
	title="Batch Delete"
	subtitle="Delete multiple files and entities in one request"
>
	<label class="field">
		<span class="field-label">File paths (one per line)</span>
		<textarea
			class="area-input"
			rows="4"
			bind:value={batchFilesText}
			spellcheck="false"></textarea>
	</label>
	<label class="field">
		<span class="field-label">Entity ids (separated by spaces or commas)</span>
		<input
			class="text-input text-input--inline"
			type="text"
			bind:value={batchEntitiesText}
			placeholder="12 34 56"
		/>
	</label>
	<div class="action-row">
		<Button
			variant={batchArmed ? 'danger' : 'secondary'}
			onclick={runBatchDelete}
			disabled={batchBusy || (!batchFilesText.trim() && !batchEntitiesText.trim())}
		>
			{#if batchBusy}Deleting...{:else if batchArmed}Confirm Batch Delete{:else}Batch
				Delete{/if}
		</Button>
	</div>
	{#if batchArmed}
		<p class="arm-warning">
			Click again to permanently delete the listed items.
		</p>
	{/if}
	{#if batchError}
		<InlineError message={batchError} />
	{/if}
	{#if batchResult}
		<div class="result-row">
			<Badge label={`${batchResult.files_deleted} files`} variant="success" />
			<Badge
				label={`${batchResult.entities_deleted} entities`}
				variant="success"
			/>
		</div>
	{/if}
</Card>

<style>
	.action-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: center;
	}

	
	
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin-bottom: 0.75rem;
	}

	.field-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

	.area-input {
		padding: 0.6rem 0.7rem;
		border: 1px solid var(--gray-300);
		background: var(--white);
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
		line-height: 1.5;
		color: var(--black);
		resize: vertical;
	}

	.area-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.arm-warning {
		margin: 0.5rem 0 0;
		color: var(--warning);
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
	}

	.result-row {
		margin-top: 0.75rem;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		color: var(--gray-600);
	}
</style>
