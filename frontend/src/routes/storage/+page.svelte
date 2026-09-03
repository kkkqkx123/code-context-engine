<script lang="ts">
	import { onMount } from 'svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { storageState, storageActions } from '$lib/stores/storage';
	import { qdrantApi, type QdrantProcessStatus } from '$lib/api/qdrant';

	let showConfirmDialog = $state(false);
	let qdrantManaged = $state(false);
	let qdrantStatus = $state<QdrantProcessStatus>('Idle');
	let qdrantLoading = $state(false);
	let qdrantError = $state<string | null>(null);

	onMount(async () => {
		storageActions.loadStatus();
		await loadQdrantStatus();
	});

	async function loadQdrantStatus() {
		try {
			const resp = await qdrantApi.getStatus();
			qdrantManaged = resp.managed;
			qdrantStatus = resp.status;
		} catch (e: any) {
			qdrantManaged = false;
		}
	}

	async function handleQdrantAction(action: 'start' | 'stop' | 'restart') {
		qdrantLoading = true;
		qdrantError = null;
		try {
			const fn = action === 'start' ? qdrantApi.start : action === 'stop' ? qdrantApi.stop : qdrantApi.restart;
			const resp = await fn();
			qdrantStatus = resp.status;
		} catch (e: any) {
			qdrantError = e.message || `Failed to ${action} Qdrant process`;
		} finally {
			qdrantLoading = false;
		}
	}

	function formatQdrantStatus(status: QdrantProcessStatus): string {
		if (typeof status === 'string') return status;
		if (status && typeof status === 'object' && 'Failed' in status) return `Failed: ${status.Failed}`;
		return String(status);
	}

	function qdrantIsRunning(): boolean {
		return qdrantStatus === 'Running';
	}

	function qdrantIsBusy(): boolean {
		return qdrantStatus === 'Starting' || qdrantStatus === 'Stopping';
	}

	function handleClearIndex() {
		showConfirmDialog = true;
	}

	async function confirmClear() {
		showConfirmDialog = false;
		await storageActions.clearIndex();
	}

	function cancelClear() {
		showConfirmDialog = false;
	}
</script>

<svelte:head>
	<title>Storage - Code Context Engine</title>
</svelte:head>

<section class="section">
	<div class="container">
		<h1>Storage Management</h1>
		<p class="page-description">Monitor storage health and manage index data</p>

		{#if $storageState.error}
			<div class="error-banner">
				<span>{$storageState.error}</span>
				<button class="dismiss-btn" onclick={storageActions.clearError}>×</button>
			</div>
		{/if}

		<!-- Storage Status Overview -->
		<Card title="Storage Components" subtitle="Health status overview">
			<div class="spec-bar">
				{#if $storageState.status}
					<div class="spec-item">
						<div class="spec-label">Vector DB</div>
						<div class="spec-value">
							<Badge
								label={$storageState.status.vector_storage.connected ? 'Connected' : 'Disconnected'}
								variant={$storageState.status.vector_storage.connected ? 'active' : 'inactive'}
							/>
						</div>
						{#if $storageState.status.vector_storage.item_count > 0}
							<div class="spec-detail">
								{$storageState.status.vector_storage.item_count} items
							</div>
						{/if}
						{#if $storageState.status.vector_storage.version}
							<div class="spec-detail version">
								v{$storageState.status.vector_storage.version}
							</div>
						{/if}
					</div>

					<div class="spec-item">
						<div class="spec-label">BM25 Index</div>
						<div class="spec-value">
							<Badge
								label={$storageState.status.bm25_storage.connected ? 'Connected' : 'Disconnected'}
								variant={$storageState.status.bm25_storage.connected ? 'active' : 'inactive'}
							/>
						</div>
						{#if $storageState.status.bm25_storage.item_count > 0}
							<div class="spec-detail">
								{$storageState.status.bm25_storage.item_count} items
							</div>
						{/if}
					</div>

					<div class="spec-item">
						<div class="spec-label">Relations</div>
						<div class="spec-value">
							<Badge
								label={$storageState.status.relation_storage.connected ? 'Connected' : 'Disconnected'}
								variant={$storageState.status.relation_storage.connected ? 'active' : 'inactive'}
							/>
						</div>
						{#if $storageState.status.relation_storage.item_count > 0}
							<div class="spec-detail">
								{$storageState.status.relation_storage.item_count} relations
							</div>
						{/if}
					</div>

					<div class="spec-item">
						<div class="spec-label">Cache</div>
						<div class="spec-value">
							<Badge
								label={$storageState.status.cache_storage.connected ? 'Connected' : 'Disconnected'}
								variant={$storageState.status.cache_storage.connected ? 'active' : 'inactive'}
							/>
						</div>
						{#if $storageState.status.cache_storage.item_count > 0}
							<div class="spec-detail">
								{$storageState.status.cache_storage.item_count} items
							</div>
						{/if}
					</div>
				{:else}
					<p class="loading-text">Loading storage status...</p>
				{/if}
			</div>

			{#if $storageState.status && $storageState.status.total_disk_usage_mb > 0}
				<div class="disk-usage">
					<span class="label">Total Disk Usage:</span>
					<span class="value">{$storageState.status.total_disk_usage_mb.toFixed(1)} MB</span>
				</div>
			{/if}
		</Card>

		<!-- Qdrant Process Management -->
		{#if qdrantManaged}
			<Card title="Qdrant Process" subtitle="Subprocess lifecycle management">
				<div class="process-info">
					<div class="process-row">
						<span class="process-label">Status</span>
						<Badge
							label={formatQdrantStatus(qdrantStatus)}
							variant={qdrantIsRunning() ? 'active' : 'inactive'}
						/>
					</div>
					{#if qdrantError}
						<div class="process-error">{qdrantError}</div>
					{/if}
				</div>
				<div class="process-actions">
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
		<Card title="Clear Index" subtitle="Remove all index data for the current project">
			<p class="clear-warning">
				This action will permanently delete all index data for the current project. This cannot be undone.
			</p>
			<div class="clear-actions">
				<Button
					onclick={handleClearIndex}
					disabled={$storageState.isLoading}
				>
					{#if $storageState.isLoading}Clearing...{:else}Clear All Index Data{/if}
				</Button>
			</div>
		</Card>
	</div>
</section>

<!-- Confirmation Dialog -->
{#if showConfirmDialog}
	<div
		class="dialog-overlay"
		onclick={cancelClear}
		onkeydown={(e) => {
			if (e.key === 'Escape') {
				cancelClear();
			}
		}}
		role="button"
		tabindex="0"
		aria-label="Close dialog"
	>
		<div
			class="dialog"
			onclick={(e) => e.stopPropagation()}
			role="dialog"
			aria-modal="true"
			aria-labelledby="dialog-title"
			aria-describedby="dialog-description"
			tabindex="-1"
			onkeydown={(e) => {
				if (e.key === 'Escape') {
					cancelClear();
				}
			}}
		>
			<h2 id="dialog-title">Confirm Clear Operation</h2>
			<p id="dialog-description" class="dialog-warning">
				This action will permanently delete all index data for the current project. This cannot be undone.
			</p>
			<div class="dialog-actions">
				<Button variant="secondary" onclick={cancelClear}>Cancel</Button>
				<Button onclick={confirmClear} disabled={$storageState.isLoading}>
					{#if $storageState.isLoading}Clearing...{:else}Confirm Clear{/if}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	h1 {
		margin-bottom: 0.5rem;
	}

	.page-description {
		font-size: 1.1rem;
		color: var(--gray-600);
		margin-bottom: 3rem;
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

	.spec-bar {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 1.5rem;
	}

	.spec-item {
		padding: 1rem;
		border: 1px solid var(--gray-200);
	}

	.spec-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		margin-bottom: 0.5rem;
	}

	.spec-value {
		margin-bottom: 0.5rem;
	}

	.spec-detail {
		font-size: 0.85rem;
		color: var(--gray-600);
	}

	.loading-text {
		color: var(--gray-400);
		font-style: italic;
		grid-column: 1 / -1;
		text-align: center;
		padding: 2rem;
	}

	.disk-usage {
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
		display: flex;
		gap: 0.5rem;
		font-size: 0.85rem;
	}

	.disk-usage .label {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: 0.1em;
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

	.process-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
		min-width: 120px;
	}

	.process-error {
		color: var(--danger);
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		margin-top: 0.5rem;
	}

	.process-actions {
		display: flex;
		gap: 0.75rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-100);
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

	.clear-actions {
		display: flex;
		justify-content: flex-end;
	}

	/* Dialog Styles */
	.dialog-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.dialog {
		background: var(--white);
		border: 2px solid var(--danger);
		padding: 2rem;
		max-width: 500px;
		width: 90%;
	}

	.dialog h2 {
		margin-bottom: 1rem;
		color: var(--black);
	}

	.dialog-warning {
		color: var(--gray-600);
		margin-bottom: 2rem;
		line-height: 1.6;
	}

	.dialog-actions {
		display: flex;
		gap: 1rem;
		justify-content: flex-end;
	}

	@media (max-width: 1024px) {
		.spec-bar {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	@media (max-width: 768px) {
		.spec-bar {
			grid-template-columns: 1fr;
		}
	}
</style>