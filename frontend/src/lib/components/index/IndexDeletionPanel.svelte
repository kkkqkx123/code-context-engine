<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import { indexApi, type DeleteFileResponse } from '$lib/api/index';
	import { projectStore } from '$lib/stores/project.svelte';

	interface Props {
		/** Called after any successful deletion so stats can be refreshed. */
		onDeleted?: () => Promise<void> | void;
	}

	let { onDeleted }: Props = $props();

	type PendingDelete =
		| { kind: 'file'; path: string }
		| { kind: 'entity'; id: number }
		| { kind: 'batch'; files: string[]; ids: number[] };

	let pendingDelete = $state<PendingDelete | null>(null);
	let busy = $state(false);

	// ─── Delete file ─────────────────────────────────────────────
	let deleteFilePath = $state('');
	let deleteFileResult = $state<DeleteFileResponse | null>(null);
	let deleteFileError = $state('');

	function requestDeleteFile() {
		const path = deleteFilePath.trim();
		if (!path) return;
		deleteFileError = '';
		pendingDelete = { kind: 'file', path };
	}

	// ─── Delete entity ───────────────────────────────────────────
	let deleteEntityId = $state('');
	let deleteEntityMessage = $state('');
	let deleteEntityError = $state('');

	function requestDeleteEntity() {
		const id = Number(deleteEntityId.trim());
		if (!Number.isFinite(id) || id <= 0) {
			deleteEntityError = 'Enter a numeric entity id';
			return;
		}
		deleteEntityError = '';
		pendingDelete = { kind: 'entity', id };
	}

	// ─── Batch delete ────────────────────────────────────────────
	let batchFilesText = $state('');
	let batchEntitiesText = $state('');
	let batchResult = $state<{
		files_deleted: number;
		entities_deleted: number;
	} | null>(null);
	let batchError = $state('');

	function requestBatchDelete() {
		const files = batchFilesText
			.split('\n')
			.map((line) => line.trim())
			.filter((line) => line.length > 0);
		const ids = batchEntitiesText
			.split(/[\s,]+/)
			.map((token) => Number(token.trim()))
			.filter((id) => Number.isFinite(id) && id > 0);
		if (files.length === 0 && ids.length === 0) {
			batchError = 'Provide at least one file path or entity id';
			return;
		}
		batchError = '';
		pendingDelete = { kind: 'batch', files, ids };
	}

	async function confirmDelete() {
		const target = pendingDelete;
		if (!target) return;
		busy = true;
		try {
			if (target.kind === 'file') {
				deleteFileError = '';
				deleteFileResult = await indexApi.deleteFile(
					target.path,
					projectStore.currentProjectId,
				);
				deleteFilePath = '';
			} else if (target.kind === 'entity') {
				deleteEntityError = '';
				const response = await indexApi.deleteEntity(
					target.id,
					projectStore.currentProjectId,
				);
				deleteEntityMessage = response.message || `Entity ${target.id} deleted`;
				deleteEntityId = '';
			} else {
				batchError = '';
				const response = await indexApi.batchDelete(
					projectStore.currentProjectId,
					{
						file_paths: target.files.length > 0 ? target.files : undefined,
						entity_ids: target.ids.length > 0 ? target.ids : undefined,
					},
				);
				batchResult = {
					files_deleted: response.files_deleted,
					entities_deleted: response.entities_deleted,
				};
				batchFilesText = '';
				batchEntitiesText = '';
			}
			await onDeleted?.();
		} catch (e) {
			const message = errorMessage(e) ?? 'Delete failed';
			if (target.kind === 'file') deleteFileError = message;
			else if (target.kind === 'entity') deleteEntityError = message;
			else batchError = message;
		} finally {
			busy = false;
			pendingDelete = null;
		}
	}

	function confirmMessage(target: PendingDelete): string {
		if (target.kind === 'file') {
			return `Permanently delete ${target.path} and all derived index data.`;
		}
		if (target.kind === 'entity') {
			return `Permanently delete entity ${target.id}.`;
		}
		return `Permanently delete ${target.files.length} file(s) and ${target.ids.length} entity id(s).`;
	}
</script>

<!--
	Destructive maintenance operations for the project workbench: single file
	delete, single entity delete and batch delete. Every action confirms in a
	modal before it runs.
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
			variant="danger"
			onclick={requestDeleteFile}
			disabled={busy || !deleteFilePath.trim()}
		>
			Delete File
		</Button>
	</div>
	{#if deleteFileError}
		<InlineError message={deleteFileError} />
	{/if}
	{#if deleteFileResult}
		<div class="result-row">
			<Badge
				label={deleteFileResult.success ? 'Deleted' : 'Failed'}
				variant={deleteFileResult.success ? 'success' : 'danger'}
			/>
			<span class="font-mono">
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
			variant="danger"
			onclick={requestDeleteEntity}
			disabled={busy || !deleteEntityId.trim()}
		>
			Delete Entity
		</Button>
	</div>
	{#if deleteEntityError}
		<InlineError message={deleteEntityError} />
	{/if}
	{#if deleteEntityMessage}
		<div class="result-row">
			<Badge label="Deleted" variant="success" />
			<span class="font-mono">{deleteEntityMessage}</span>
		</div>
	{/if}
</Card>

<Card
	title="Batch Delete"
	subtitle="Delete multiple files and entities in one request"
>
	<label class="field">
		<span class="form-label">File paths (one per line)</span>
		<textarea
			class="text-input"
			rows="4"
			bind:value={batchFilesText}
			spellcheck="false"></textarea>
	</label>
	<label class="field">
		<span class="form-label">Entity ids (separated by spaces or commas)</span>
		<input
			class="text-input text-input--inline"
			type="text"
			bind:value={batchEntitiesText}
			placeholder="12 34 56"
		/>
	</label>
	<div class="action-row">
		<Button
			variant="danger"
			onclick={requestBatchDelete}
			disabled={busy || (!batchFilesText.trim() && !batchEntitiesText.trim())}
		>
			Batch Delete
		</Button>
	</div>
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

{#if pendingDelete}
	<Modal
		title="Confirm delete?"
		onClose={() => {
			if (!busy) pendingDelete = null;
		}}
	>
		<p>{confirmMessage(pendingDelete)}</p>
		{#snippet actions()}
			<Button
				variant="secondary"
				onclick={() => (pendingDelete = null)}
				disabled={busy}
			>
				Cancel
			</Button>
			<Button variant="danger" onclick={confirmDelete} disabled={busy}>
				{busy ? 'Deleting...' : 'Confirm Delete'}
			</Button>
		{/snippet}
	</Modal>
{/if}

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
		gap: 0.25rem;
		margin-bottom: 0.75rem;
	}

	.result-row {
		margin-top: 0.75rem;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}
</style>
