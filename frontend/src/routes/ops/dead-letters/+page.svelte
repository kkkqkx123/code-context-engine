<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import ErrorBanner from '$lib/components/ui/ErrorBanner.svelte';
	import {
		deadLetterApi,
		type DeadLetterFileEntry,
	} from '$lib/api/dead-letters';
	import { deadLetterGuidance } from '$lib/dead-letter-actions';
	import { toastStore } from '$lib/stores/toast.svelte';
	import { ApiError } from '$lib/api/client';
	import { projectStore } from '$lib/stores/project.svelte';

	const REFRESH_MS = 30_000;

	let projectId = $state<number | null>(null);
	let files = $state<DeadLetterFileEntry[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let selected = new SvelteSet<string>();
	let ackTarget = $state<DeadLetterFileEntry | null>(null);
	let expanded = new SvelteSet<string>();
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// Follow the global project selection (sidebar/topbar ProjectSelector).
	let lastLoadedProject: number | null = null;
	$effect(() => {
		const id = projectStore.currentProjectId;
		if (id !== lastLoadedProject) {
			lastLoadedProject = id;
			projectId = id;
			selected.clear();
			expanded.clear();
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
			for (const p of [...selected]) {
				if (!paths.has(p)) selected.delete(p);
			}
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
		if (selected.has(path)) selected.delete(path);
		else selected.add(path);
	}

	function toggleSelectAll() {
		if (selected.size === files.length) {
			selected.clear();
		} else {
			selected.clear();
			for (const f of files) selected.add(f.file_path);
		}
	}

	function toggleExpand(path: string) {
		if (expanded.has(path)) expanded.delete(path);
		else expanded.add(path);
	}

	async function runRetry(paths: string[]) {
		if (projectId === null) return;
		busy = true;
		try {
			const res = await deadLetterApi.retry(projectId, paths);
			toastStore.show(res.message, 'success');
			await load();
		} catch (e) {
			toastStore.show(e instanceof ApiError ? e.message : String(e), 'error');
		} finally {
			busy = false;
		}
	}

	async function runAcknowledge(path: string, module?: string) {
		if (projectId === null) return;
		busy = true;
		try {
			const res = await deadLetterApi.acknowledge(projectId, path, module);
			toastStore.show(res.message, 'success');
			await load();
		} catch (e) {
			toastStore.show(e instanceof ApiError ? e.message : String(e), 'error');
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
				failures.push(
					`${path}: ${e instanceof ApiError ? e.message : String(e)}`,
				);
			}
		}
		busy = false;
		if (failures.length === 0) {
			toastStore.show(`Acknowledged ${selected.size} file(s)`, 'success');
		} else {
			toastStore.show(
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
			<Button
				variant="secondary"
				onclick={load}
				disabled={loading || projectId === null}
			>
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
			<ErrorBanner message={error} />
		{/if}

		{#if projectId === null}
			<Card
				><p class="empty">
					Select a project to inspect its dead letters.
				</p></Card
			>
		{:else if loading && files.length === 0}
			<Card><p class="empty">Loading dead letters…</p></Card>
		{:else if files.length === 0}
			<Card
				><p class="empty">
					No dead letters. All index modules are healthy.
				</p></Card
			>
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

				<div class="table-wrap">
					<table class="table">
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
										<button
											type="button"
											class="file-link"
											onclick={() => toggleExpand(file.file_path)}
										>
											{file.file_path}
										</button>
										<span class="version">v{file.version}</span>
									</td>
									<td class="modules">
										{#each file.modules as m (m.module)}
											<Badge variant={m.acknowledged ? 'inactive' : 'warning'}>
												{m.module} ×{m.retry_count}
												{#if m.truncated}
													(truncated){/if}
												{#if m.acknowledged}
													(acked){/if}
											</Badge>
										{/each}
									</td>
									<td class="guidance">{guidance.hint}</td>
									<td class="actions">
										<Button
											variant={guidance.primary === 'retry'
												? 'primary'
												: 'secondary'}
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
													<span class="acked-flag"
														>{m.acknowledged ? 'acknowledged' : 'pending'}</span
													>
													<p>{m.error_message ?? '(no message)'}</p>
												</div>
											{/each}
										</td>
									</tr>
								{/if}
							{/each}
						</tbody>
					</table>
				</div>
			</Card>
		{/if}
	</div>
</div>

{#if ackTarget}
	<Modal title="Acknowledge dead letter?" onClose={() => (ackTarget = null)}>
		<p>{ackConfirmMessage()}</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (ackTarget = null)}
				>Cancel</Button
			>
			<Button
				variant="danger"
				disabled={busy}
				onclick={() => runAcknowledge(ackTarget!.file_path)}>Acknowledge</Button
			>
		{/snippet}
	</Modal>
{/if}

<style>
	.empty {
		color: var(--ops-text-muted);
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
	.file-link {
		background: none;
		border: none;
		color: var(--ops-link);
		cursor: pointer;
		padding: 0;
		font: inherit;
		word-break: break-all;
		text-decoration: underline;
	}
	.version {
		margin-left: 0.4rem;
		color: var(--ops-text-muted);
		font-size: 0.75rem;
	}
	.modules {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		max-width: 220px;
	}
	.guidance {
		max-width: 260px;
		color: var(--ops-text-muted);
	}
	.actions {
		display: flex;
		gap: 0.4rem;
		white-space: nowrap;
	}
	.detail-row td {
		background: var(--ops-surface-raised);
	}
	.module-detail {
		margin-bottom: 0.5rem;
	}
	.module-detail .code {
		margin-left: 0.5rem;
		font-family: monospace;
		font-size: 0.75rem;
		color: var(--warning);
	}
	.module-detail .acked-flag {
		margin-left: 0.5rem;
		font-size: 0.75rem;
		color: var(--ops-text-muted);
	}
	.module-detail p {
		margin: 0.2rem 0 0;
		color: var(--ops-text-muted);
	}
</style>
