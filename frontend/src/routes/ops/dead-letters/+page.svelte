<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { deadLetterApi, type DeadLetterFileEntry } from '$lib/api/dead-letters';
	import { deadLetterGuidance } from '$lib/dead-letter-actions';
	import { toastActions } from '$lib/stores/toast';
	import { ApiError } from '$lib/api/client';
	import { currentProjectId } from '$lib/stores/project';

	const REFRESH_MS = 30_000;

	let projectId = $state<number | null>(null);
	let files = $state<DeadLetterFileEntry[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let selected = $state<Set<string>>(new Set());
	let ackTarget = $state<DeadLetterFileEntry | null>(null);
	let expanded = $state<Set<string>>(new Set());
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// Follow the global project selection (sidebar/topbar ProjectSelector).
	let lastLoadedProject: number | null = null;
	$effect(() => {
		const id = $currentProjectId;
		if (id !== lastLoadedProject) {
			lastLoadedProject = id;
			projectId = id;
			selected = new Set();
			expanded = new Set();
			load();
		}
	});

	async function load() {
		if (projectId === null) {
			files = [];
			return;
		}
		loading = true;
		error = null;
		try {
			const res = await deadLetterApi.list(projectId);
			files = res.files;
			// Drop selections that no longer exist in the list.
			const paths = new Set(files.map((f) => f.file_path));
			selected = new Set([...selected].filter((p) => paths.has(p)));
		} catch (e) {
			error = e instanceof ApiError ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	function startPolling() {
		stopPolling();
		pollTimer = setInterval(load, REFRESH_MS);
	}

	function stopPolling() {
		if (pollTimer !== null) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	function toggleSelect(path: string) {
		const next = new Set(selected);
		if (next.has(path)) next.delete(path);
		else next.add(path);
		selected = next;
	}

	function toggleSelectAll() {
		selected =
			selected.size === files.length ? new Set() : new Set(files.map((f) => f.file_path));
	}

	function toggleExpand(path: string) {
		const next = new Set(expanded);
		if (next.has(path)) next.delete(path);
		else next.add(path);
		expanded = next;
	}

	function reportMessage(m: {
		retried?: number;
		succeeded?: number;
		still_failed?: number;
		truncated_chunks?: number;
		affected?: number;
	}) {
		if (m.retried !== undefined) {
			return `retried ${m.retried}, succeeded ${m.succeeded}, still failed ${m.still_failed}, chunks truncated ${m.truncated_chunks}`;
		}
		return `acknowledged ${m.affected} module(s)`;
	}

	async function runRetry(paths: string[]) {
		if (projectId === null) return;
		busy = true;
		try {
			const res = await deadLetterApi.retry(projectId, paths);
			toastActions.show(res.message, 'success');
			await load();
		} catch (e) {
			toastActions.show(e instanceof ApiError ? e.message : String(e), 'error');
		} finally {
			busy = false;
		}
	}

	async function runAcknowledge(path: string, module?: string) {
		if (projectId === null) return;
		busy = true;
		try {
			const res = await deadLetterApi.acknowledge(projectId, path, module);
			toastActions.show(res.message, 'success');
			await load();
		} catch (e) {
			toastActions.show(e instanceof ApiError ? e.message : String(e), 'error');
		} finally {
			busy = false;
			ackTarget = null;
		}
	}

	async function acknowledgeSelected() {
		if (projectId === null || selected.size === 0) return;
		busy = true;
		const failures: string[] = [];
		for (const path of selected) {
			try {
				await deadLetterApi.acknowledge(projectId, path);
			} catch (e) {
				failures.push(`${path}: ${e instanceof ApiError ? e.message : String(e)}`);
			}
		}
		busy = false;
		if (failures.length === 0) {
			toastActions.show(`Acknowledged ${selected.size} file(s)`, 'success');
		} else {
			toastActions.show(
				`Acknowledged ${selected.size - failures.length}, failed ${failures.length}: ${failures.join('; ')}`,
				'warning',
				10_000,
			);
		}
		await load();
	}

	function ackConfirmMessage(): string {
		if (!ackTarget) return '';
		const modules = ackTarget.modules
			.filter((m) => !m.acknowledged)
			.map((m) => m.module)
			.join(', ');
		return `Acknowledged dead letters no longer participate in automatic or manual retry passes. File: ${ackTarget.file_path} (modules: ${modules})`;
	}

	onMount(() => {
		load();
		startPolling();
	});
	onDestroy(stopPolling);
</script>

<svelte:head>
	<title>Dead Letters - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<PageHeader
			title="Index Dead Letters"
			subtitle="Permanently failed index modules requiring manual intervention"
		>
			<Button variant="secondary" onclick={load} disabled={loading || projectId === null}>
				Refresh
			</Button>
			<Button
				variant="primary"
				onclick={() => runRetry([])}
				disabled={busy || projectId === null || files.length === 0}
			>
				Retry All
			</Button>
		</PageHeader>

		{#if error}
			<div class="error-banner">{error}</div>
		{/if}

		{#if projectId === null}
			<Card><p class="empty">Select a project to inspect its dead letters.</p></Card>
		{:else if loading && files.length === 0}
			<Card><p class="empty">Loading dead letters…</p></Card>
		{:else if files.length === 0}
			<Card><p class="empty">No dead letters. All index modules are healthy.</p></Card>
		{:else}
			<Card>
				<div class="toolbar">
					<label class="select-all">
						<input
							type="checkbox"
							checked={selected.size === files.length}
							onchange={toggleSelectAll}
						/>
						Select all ({files.length})
					</label>
					<span class="spacer"></span>
					<Button
						variant="primary"
						disabled={busy || selected.size === 0}
						onclick={() => runRetry([...selected])}
					>
						Retry Selected ({selected.size})
					</Button>
					<Button
						variant="danger"
						disabled={busy || selected.size === 0}
						onclick={acknowledgeSelected}
					>
						Acknowledge Selected
					</Button>
				</div>

				<table class="dead-table">
					<thead>
						<tr>
							<th></th>
							<th>File</th>
							<th>Modules</th>
							<th>Guidance</th>
							<th>Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each files as file (file.file_path)}
							{@const pending = file.modules.filter((m) => !m.acknowledged)}
							{@const firstCode = pending[0]?.error_code}
							{@const guidance = deadLetterGuidance(firstCode)}
							<tr>
								<td>
									<input
										type="checkbox"
										checked={selected.has(file.file_path)}
										onchange={() => toggleSelect(file.file_path)}
									/>
								</td>
								<td>
									<button class="file-link" onclick={() => toggleExpand(file.file_path)}>
										{file.file_path}
									</button>
									<span class="version">v{file.version}</span>
								</td>
								<td class="modules">
									{#each file.modules as m (m.module)}
										<span class="module-chip" class:acked={m.acknowledged}>
											{m.module} ×{m.retry_count}
											{#if m.truncated}<em title="already truncate-retried">T</em>{/if}
											{#if m.acknowledged}<em title="acknowledged">✓</em>{/if}
										</span>
									{/each}
								</td>
								<td class="guidance">{guidance.hint}</td>
								<td class="actions">
									<Button
										variant={guidance.primary === 'retry' ? 'primary' : 'secondary'}
										disabled={busy || pending.length === 0}
										onclick={() => runRetry([file.file_path])}
									>
										Retry
									</Button>
									<Button
										variant="secondary"
										disabled={busy || pending.length === 0}
										onclick={() => (ackTarget = file)}
									>
										Acknowledge
									</Button>
								</td>
							</tr>
							{#if expanded.has(file.file_path)}
								<tr class="detail-row">
									<td></td>
									<td colspan="4">
										{#each file.modules as m (m.module)}
											<div class="module-detail">
												<strong>{m.module}</strong>
												<span class="code">{m.error_code ?? 'NO_CODE'}</span>
												<span class="acked-flag">{m.acknowledged ? 'acknowledged' : 'pending'}</span>
												<p>{m.error_message ?? '(no message)'}</p>
											</div>
										{/each}
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</Card>
		{/if}
	</div>
</div>

{#if ackTarget}
	<div class="modal-backdrop" role="presentation" onclick={() => (ackTarget = null)}>
		<div class="modal" role="dialog" aria-modal="true">
			<h3>Acknowledge dead letter?</h3>
			<p>{ackConfirmMessage()}</p>
			<div class="modal-actions">
				<Button variant="secondary" onclick={() => (ackTarget = null)}>Cancel</Button>
				<Button variant="danger" disabled={busy} onclick={() => runAcknowledge(ackTarget!.file_path)}>
					Acknowledge
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.error-banner {
		padding: 0.75rem 1rem;
		border-radius: 6px;
		background: var(--color-danger-bg, #3b1a1a);
		color: var(--color-danger, #ff6b6b);
		margin-bottom: 1rem;
	}
	.empty {
		color: var(--color-text-muted, #888);
		text-align: center;
		padding: 2rem 0;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}
	.toolbar .spacer {
		flex: 1;
	}
	.select-all {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.85rem;
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
		vertical-align: top;
	}
	.file-link {
		background: none;
		border: none;
		color: var(--color-link, #6cb2ff);
		cursor: pointer;
		padding: 0;
		font: inherit;
		word-break: break-all;
	}
	.version {
		margin-left: 0.4rem;
		color: var(--color-text-muted, #888);
		font-size: 0.75rem;
	}
	.modules {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		max-width: 220px;
	}
	.module-chip {
		display: inline-flex;
		gap: 0.25rem;
		align-items: center;
		padding: 0.1rem 0.45rem;
		border-radius: 999px;
		background: var(--color-surface-raised, #2a2a2a);
		font-size: 0.75rem;
	}
	.module-chip.acked {
		opacity: 0.5;
	}
	.module-chip em {
		font-style: normal;
		font-size: 0.7rem;
		color: var(--color-warning, #e6b450);
	}
	.guidance {
		max-width: 260px;
		color: var(--color-text-muted, #aaa);
	}
	.actions {
		display: flex;
		gap: 0.4rem;
		white-space: nowrap;
	}
	.detail-row td {
		background: var(--color-surface-raised, #222);
	}
	.module-detail {
		margin-bottom: 0.5rem;
	}
	.module-detail .code {
		margin-left: 0.5rem;
		font-family: monospace;
		font-size: 0.75rem;
		color: var(--color-warning, #e6b450);
	}
	.module-detail .acked-flag {
		margin-left: 0.5rem;
		font-size: 0.75rem;
		color: var(--color-text-muted, #888);
	}
	.module-detail p {
		margin: 0.2rem 0 0;
		color: var(--color-text-muted, #aaa);
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
		max-width: 460px;
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
