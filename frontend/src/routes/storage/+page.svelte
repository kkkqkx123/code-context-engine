<script lang="ts">
	import { errorMessage } from '$lib/utils/errors';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import { storageStore } from '$lib/stores/storage.svelte';
	import { qdrantApi, type QdrantProcessStatus } from '$lib/api/qdrant';

	let showConfirmDialog = $state(false);
	let qdrantManaged = $state(false);
	let qdrantStatus = $state<QdrantProcessStatus>({ type: 'Idle' });
	let qdrantLoading = $state(false);
	let qdrantError = $state<string | null>(null);

	onMount(async () => {
		storageStore.loadStatus();
		await loadQdrantStatus();
	});

	async function loadQdrantStatus() {
		try {
			const resp = await qdrantApi.getStatus();
			qdrantManaged = resp.managed;
			qdrantStatus = resp.status;
		} catch {
			qdrantManaged = false;
		}
	}

	async function handleQdrantAction(action: 'start' | 'stop' | 'restart') {
		qdrantLoading = true;
		qdrantError = null;
		try {
			const fn =
				action === 'start'
					? qdrantApi.start
					: action === 'stop'
						? qdrantApi.stop
						: qdrantApi.restart;
			const resp = await fn();
			qdrantStatus = resp.status;
		} catch (e) {
			qdrantError = errorMessage(e) || `Failed to ${action} Qdrant process`;
		} finally {
			qdrantLoading = false;
		}
	}

	function backendName(
		component: { backend?: string | null } | null | undefined,
	): string {
		return component?.backend?.trim() ? String(component.backend) : 'unknown';
	}

	function formatQdrantStatus(status: QdrantProcessStatus): string {
		if (status.type === 'Failed') return `Failed: ${status.message}`;
		return status.type;
	}

	function qdrantIsRunning(): boolean {
		return qdrantStatus.type === 'Running';
	}

	function qdrantIsBusy(): boolean {
		return qdrantStatus.type === 'Starting' || qdrantStatus.type === 'Stopping';
	}

	function handleClearIndex() {
		showConfirmDialog = true;
	}

	async function confirmClear() {
		showConfirmDialog = false;
		await storageStore.clearIndex();
	}

	function cancelClear() {
		showConfirmDialog = false;
	}
</script>

<svelte:head>
	<title>Storage - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Storage Management"
			subtitle="Monitor storage health and manage index data"
		/>

		{#if storageStore.error}
			<ErrorBanner
				message={storageStore.error}
				onDismiss={storageStore.clearError}
			/>
		{/if}

		<!-- Storage Status Overview -->
		<Card title="Storage Components" subtitle="Health status overview">
			<p class="backend-note">
				Active backends are read-only. Switching backends requires editing the
				server configuration file, then restarting and performing a full
				reindex.
			</p>
			<div class="stat-grid">
				{#if storageStore.status}
					<div class="spec-item">
						<div class="form-label">
							Vector DB ({backendName(storageStore.status.vector_storage)})
						</div>
						<div class="spec-value">
							<Badge
								label={storageStore.status.vector_storage.connected
									? 'Connected'
									: 'Disconnected'}
								variant={storageStore.status.vector_storage.connected
									? 'success'
									: 'danger'}
							/>
						</div>
						{#if storageStore.status.vector_storage.item_count > 0}
							<div class="spec-detail">
								{storageStore.status.vector_storage.item_count} items
							</div>
						{/if}
						{#if storageStore.status.vector_storage.version}
							<div class="spec-detail version">
								v{storageStore.status.vector_storage.version}
							</div>
						{/if}
					</div>

					<div class="spec-item">
						<div class="form-label">
							BM25 Index ({backendName(storageStore.status.bm25_storage)})
						</div>
						<div class="spec-value">
							<Badge
								label={storageStore.status.bm25_storage.connected
									? 'Connected'
									: 'Disconnected'}
								variant={storageStore.status.bm25_storage.connected
									? 'success'
									: 'danger'}
							/>
						</div>
						{#if storageStore.status.bm25_storage.item_count > 0}
							<div class="spec-detail">
								{storageStore.status.bm25_storage.item_count} items
							</div>
						{/if}
					</div>

					<div class="spec-item">
						<div class="form-label">
							Relations ({backendName(storageStore.status.relation_storage)})
						</div>
						<div class="spec-value">
							<Badge
								label={storageStore.status.relation_storage.connected
									? 'Connected'
									: 'Disconnected'}
								variant={storageStore.status.relation_storage.connected
									? 'success'
									: 'danger'}
							/>
						</div>
						{#if storageStore.status.relation_storage.item_count > 0}
							<div class="spec-detail">
								{storageStore.status.relation_storage.item_count} relations
							</div>
						{/if}
					</div>
				{:else}
					<LoadingState message="Loading storage status..." />
				{/if}
			</div>

			{#if storageStore.status && storageStore.status.total_disk_usage_mb > 0}
				<div class="disk-usage">
					<span class="label">Total Disk Usage:</span>
					<span class="value"
						>{storageStore.status.total_disk_usage_mb.toFixed(1)} MB</span
					>
				</div>
			{/if}
		</Card>

		<!-- Qdrant Process Management -->
		{#if qdrantManaged}
			<Card title="Qdrant Process" subtitle="Subprocess lifecycle management">
				<div class="process-info">
					<div class="process-row">
						<span class="form-label">Status</span>
						<Badge
							label={formatQdrantStatus(qdrantStatus)}
							variant={qdrantStatus.type === 'Failed'
								? 'danger'
								: qdrantIsRunning()
									? 'success'
									: qdrantIsBusy()
										? 'warning'
										: 'inactive'}
						/>
					</div>
					{#if qdrantError}
						<div class="process-error">{qdrantError}</div>
					{/if}
				</div>
				<div class="tool-actions">
					<Button
						variant="secondary"
						onclick={() => handleQdrantAction('start')}
						disabled={qdrantLoading || qdrantIsRunning() || qdrantIsBusy()}
					>
						Start
					</Button>
					<Button
						variant="secondary"
						onclick={() => handleQdrantAction('stop')}
						disabled={qdrantLoading || !qdrantIsRunning() || qdrantIsBusy()}
					>
						Stop
					</Button>
					<Button
						variant="secondary"
						onclick={() => handleQdrantAction('restart')}
						disabled={qdrantLoading || qdrantIsBusy()}
					>
						Restart
					</Button>
				</div>
			</Card>
		{/if}

		<!-- Clear Index -->
		<Card
			title="Clear Index"
			subtitle="Remove all index data for the current project"
		>
			<p class="clear-warning">
				This action will permanently delete all index data for the current
				project. This cannot be undone.
			</p>
			<div class="reload-actions">
				<Button onclick={handleClearIndex} disabled={storageStore.isLoading}>
					{#if storageStore.isLoading}Clearing...{:else}Clear All Index Data{/if}
				</Button>
			</div>
		</Card>
	</div>
</div>

{#if showConfirmDialog}
	<Modal title="Confirm Clear Operation" onClose={cancelClear}>
		<p>
			This action will permanently delete all index data for the current
			project. This cannot be undone.
		</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={cancelClear}>Cancel</Button>
			<Button onclick={confirmClear} disabled={storageStore.isLoading}>
				{#if storageStore.isLoading}Clearing...{:else}Confirm Clear{/if}
			</Button>
		{/snippet}
	</Modal>
{/if}

<style>
	.backend-note {
		color: var(--gray-600);
		font-size: 0.85rem;
		margin-bottom: 1rem;
		line-height: 1.6;
	}

	.spec-item {
		padding: 1rem;
		border: var(--border-muted);
	}

	.spec-value {
		margin-bottom: 0.5rem;
	}

	.spec-detail {
		font-size: 0.85rem;
		color: var(--gray-600);
	}

	.disk-usage {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: var(--border-muted);
		display: flex;
		gap: 0.5rem;
		font-size: 0.85rem;
	}

	.disk-usage .label {
		font-family: var(--font-mono);
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: var(--tracking-widest);
		color: var(--gray-600);
	}

	.disk-usage .value {
		font-weight: 700;
	}

	.process-info {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.process-row {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--gray-100);
	}

	.process-row:last-child {
		border-bottom: none;
	}

	.process-error {
		color: var(--danger);
		font-family: var(--font-mono);
		font-size: 0.75rem;
		margin-top: 0.5rem;
	}

	.version {
		font-size: 0.75rem;
		color: var(--gray-500);
	}

	.clear-warning {
		color: var(--gray-600);
		margin-bottom: 1.5rem;
		line-height: 1.6;
	}
</style>
