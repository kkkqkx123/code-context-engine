<script lang="ts">
	import { onMount } from 'svelte';
	import { logsApi, type LogFileInfo } from '$lib/api/logs';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Toolbar from '$lib/components/ui/Toolbar.svelte';
	import InlineError from '$lib/components/ui/InlineError.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';

	let files = $state<LogFileInfo[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function refresh() {
		loading = true;
		error = null;
		try {
			files = await logsApi.listFiles();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function rotate() {
		try {
			await logsApi.rotate();
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	async function remove(name: string) {
		if (!confirm(`Delete ${name}?`)) return;
		try {
			await logsApi.deleteFile(name);
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(refresh);

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<Card title="Log Files" subtitle="Rotate, download and clean up log files">
	{#if error}
		<InlineError message={error} />
	{/if}
	<Toolbar>
		<span class="count">{files.length} file(s)</span>
		{#snippet actions()}
			<Button variant="secondary" onclick={refresh} disabled={loading}>
				{loading ? 'Loading...' : 'Refresh'}
			</Button>
			<Button variant="secondary" onclick={rotate}>Rotate Now</Button>
		{/snippet}
	</Toolbar>
	{#if files.length === 0}
		<EmptyState message="No log files found." />
	{:else}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Name</th>
						<th>Size</th>
						<th>Modified</th>
						<th>Actions</th>
					</tr>
				</thead>
				<tbody>
					{#each files as f (f.name)}
						<tr>
							<td class="mono">{f.name}</td>
							<td class="mono">{formatSize(f.size)}</td>
							<td class="mono">{f.modified}</td>
							<td>
								<div class="row-actions">
									<a
										class="download-link"
										href={logsApi.downloadUrl(f.name)}
										target="_blank"
										rel="external">Download</a
									>
									<Button
										variant="danger"
										size="xs"
										onclick={() => remove(f.name)}
									>
										Delete
									</Button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</Card>

<style>
	.count {
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		color: var(--gray-600);
	}

	.mono {
		font-family: 'Space Mono', monospace;
		font-size: 0.78rem;
	}

	.row-actions {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.download-link {
		font-family: 'Space Mono', monospace;
		font-size: 0.72rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--info);
		text-decoration: none;
		border-bottom: 1px solid var(--info);
	}
</style>
