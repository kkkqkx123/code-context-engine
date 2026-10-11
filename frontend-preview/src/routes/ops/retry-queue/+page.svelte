<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import {
		retryQueueApi,
		type RetryQueueStatus,
		type RetryQueueDeadEntry,
	} from '$lib/api/dead-letters';
	import { toastActions } from '$lib/stores/toast';
	import { ApiError } from '$lib/api/client';
	import { onMount } from 'svelte';

	const REFRESH_MS = 30_000;

	let status = $state<RetryQueueStatus | null>(null);
	let deadEntries = $state<RetryQueueDeadEntry[]>([]);
	let loading = $state(false);
	let busy = $state(false);
	let clearConfirmOpen = $state(false);
	let error = $state<string | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			status = await retryQueueApi.getStatus();
			const dead = await retryQueueApi.getDead();
			deadEntries = dead.entries;
		} catch (e) {
			error = e instanceof ApiError ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function clearDead() {
		busy = true;
		try {
			const res = await retryQueueApi.clearDead();
			toastActions.show(res.message, 'success');
			clearConfirmOpen = false;
			await load();
		} catch (e) {
			toastActions.show(e instanceof ApiError ? e.message : String(e), 'error');
		} finally {
			busy = false;
		}
	}

	function deadBadge(count: number | undefined) {
		if (count === undefined) return { label: 'Unknown', variant: 'inactive' as const };
		return count > 0
			? { label: `${count} dead`, variant: 'warning' as const }
			: { label: 'No dead', variant: 'success' as const };
	}

	onMount(() => {
		load();
		const timer = setInterval(load, REFRESH_MS);
		return () => clearInterval(timer);
	});
</script>

<svelte:head>
	<title>Retry Queue - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Query Retry Queue"
			subtitle="Queued query replays and dead-lettered queries across all projects"
		>
			<Button variant="secondary" onclick={load} disabled={loading}>Refresh</Button>
		</PageHeader>

		{#if error}
			<div class="error-banner">{error}</div>
		{/if}

		{#if status}
			<div class="stats-row">
				<Card>
					<div class="stat">
						<span class="stat-value">{status.pending_count}</span>
						<span class="stat-label">Pending replay</span>
					</div>
				</Card>
				<Card>
					<div class="stat">
						<span class="stat-value">{status.dead_count}</span>
						<span class="stat-label">Dead queries</span>
						<Badge {...deadBadge(status.dead_count)} />
					</div>
				</Card>
			</div>
		{/if}

		<Card>
			<div class="toolbar">
				<h2>Dead-lettered queries</h2>
				<span class="spacer"></span>
				<Button
					variant="danger"
					disabled={busy || deadEntries.length === 0}
					onclick={() => (clearConfirmOpen = true)}
				>
					Clear Dead List
				</Button>
			</div>
			<p class="note">
				Dead queries exceeded the maximum retry count and are never replayed
				automatically. Re-issue them manually once the backend service has recovered.
			</p>
			{#if deadEntries.length === 0}
				<p class="empty">No dead-lettered queries.</p>
			{:else}
				<table class="dead-table">
					<thead>
						<tr>
							<th>Query</th>
							<th>Retries</th>
						</tr>
					</thead>
					<tbody>
						{#each deadEntries as entry, i (i)}
							<tr>
								<td class="query">{entry.query}</td>
								<td>{entry.retry_count}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</Card>
	</div>
</div>

{#if clearConfirmOpen}
	<div class="modal-backdrop" role="presentation" onclick={() => (clearConfirmOpen = false)}>
		<div class="modal" role="dialog" aria-modal="true">
			<h3>Clear the dead list?</h3>
			<p>
				All {deadEntries.length} dead-lettered query entries will be discarded. This cannot
				be undone.
			</p>
			<div class="modal-actions">
				<Button variant="secondary" onclick={() => (clearConfirmOpen = false)}>Cancel</Button>
				<Button variant="danger" disabled={busy} onclick={clearDead}>Clear</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.stats-row {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 1rem;
		margin-bottom: 1rem;
	}
	.stat {
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
	}
	.stat-value {
		font-size: 1.8rem;
		font-weight: 600;
	}
	.stat-label {
		color: var(--color-text-muted, #888);
	}
	.error-banner {
		padding: 0.75rem 1rem;
		border-radius: 6px;
		background: var(--color-danger-bg, #3b1a1a);
		color: var(--color-danger, #ff6b6b);
		margin-bottom: 1rem;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.toolbar h2 {
		margin: 0;
		font-size: 1rem;
	}
	.toolbar .spacer {
		flex: 1;
	}
	.note {
		color: var(--color-text-muted, #888);
		font-size: 0.8rem;
		margin: 0 0 1rem;
	}
	.empty {
		color: var(--color-text-muted, #888);
		text-align: center;
		padding: 2rem 0;
	}
	.dead-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
	}
	.dead-table th,
	.dead-table td {
		text-align: left;
		padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--color-border, #333);
	}
	.query {
		word-break: break-all;
	}
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}
	.modal {
		background: var(--color-surface, #1e1e1e);
		border-radius: 8px;
		padding: 1.25rem 1.5rem;
		max-width: 420px;
		width: 90%;
	}
	.modal h3 {
		margin: 0 0 0.5rem;
	}
	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 1rem;
	}
</style>
