<script lang="ts">
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import {
		retryQueueApi,
		type RetryQueueStatus,
		type RetryQueueDeadEntry,
	} from '$lib/api/dead-letters';
	import { toastStore } from '$lib/stores/toast.svelte';
	import { ApiError } from '$lib/api/client';
	import { onMount } from 'svelte';
	import { projectStore } from '$lib/stores/project.svelte';

	const REFRESH_MS = 30_000;

	let status = $state<RetryQueueStatus | null>(null);
	let deadEntries = $state<RetryQueueDeadEntry[]>([]);
	let loading = $state(false);
	let busy = $state(false);
	let clearConfirmOpen = $state(false);
	let error = $state<string | null>(null);
	let selectedProjectId = $state<number | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			status = await retryQueueApi.getStatus(selectedProjectId);
			const dead = await retryQueueApi.getDead(selectedProjectId);
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
			const res = await retryQueueApi.clearDead(selectedProjectId);
			toastStore.show(res.message, 'success');
			clearConfirmOpen = false;
			await load();
		} catch (e) {
			toastStore.show(e instanceof ApiError ? e.message : String(e), 'error');
		} finally {
			busy = false;
		}
	}

	function deadBadge(count: number | undefined) {
		if (count === undefined)
			return { label: 'Unknown', variant: 'inactive' as const };
		return count > 0
			? { label: `${count} dead`, variant: 'warning' as const }
			: { label: 'No dead', variant: 'success' as const };
	}

	onMount(() => {
		projectStore.loadProjects();
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
			<Button variant="secondary" onclick={load} disabled={loading}
				>Refresh</Button
			>
		</PageHeader>

		{#if error}
			<ErrorBanner message={error} />
		{/if}

		<div class="filter-row">
			<label for="retry-project-scope">Project scope</label>
			<select
				id="retry-project-scope"
				class="text-input"
				onchange={(e) => {
					const value = (e.currentTarget as HTMLSelectElement).value;
					selectedProjectId = value === 'all' ? null : Number(value);
					load();
				}}
			>
				<option value="all" selected={selectedProjectId === null}
					>All projects</option
				>
				{#each projectStore.projects as project (project.id)}
					<option
						value={Number(project.id)}
						selected={selectedProjectId === Number(project.id)}
					>
						{project.name || `#${project.id}`}
					</option>
				{/each}
			</select>
		</div>

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
				automatically. Re-issue them manually once the backend service has
				recovered.
			</p>
			{#if deadEntries.length === 0}
				<p class="empty">No dead-lettered queries.</p>
			{:else}
				<div class="table-wrap">
					<table class="table">
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
				</div>
			{/if}
		</Card>
	</div>
</div>

{#if clearConfirmOpen}
	<Modal
		title="Clear the dead list?"
		onClose={() => (clearConfirmOpen = false)}
	>
		<p>
			All {deadEntries.length} dead-lettered query entries will be discarded. This
			cannot be undone.
		</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (clearConfirmOpen = false)}
				>Cancel</Button
			>
			<Button variant="danger" disabled={busy} onclick={clearDead}>Clear</Button
			>
		{/snippet}
	</Modal>
{/if}

<style>
	.filter-row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin-bottom: 1rem;
	}
	.filter-row label {
		font-size: 0.8rem;
		color: var(--ops-text-muted);
	}
	.filter-row select {
		padding: 0.35rem 0.5rem;
	}
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
		color: var(--ops-text-muted);
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
		color: var(--ops-text-muted);
		font-size: 0.8rem;
		margin: 0 0 1rem;
	}
	.empty {
		color: var(--ops-text-muted);
		text-align: center;
		padding: 2rem 0;
	}
	.query {
		word-break: break-all;
	}
</style>
