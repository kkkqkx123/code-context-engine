<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import { healthStore } from '$lib/stores/health.svelte';

	/**
	 * Retry queue card: pending write-behind operations with process and
	 * clear actions. Owns its confirmation dialog and busy state.
	 */
	let clearConfirmOpen = $state(false);
	let actionBusy = $state(false);

	async function runAction(action: () => Promise<unknown>) {
		actionBusy = true;
		try {
			await action();
		} finally {
			actionBusy = false;
			clearConfirmOpen = false;
		}
	}
</script>

<Card title="Retry Queue" subtitle="Pending write-behind operations">
	{#if healthStore.retryQueue}
		<div class="retry-row">
			<div class="retry-stat">
				<span class="form-label">Pending operations</span>
				<span class="stat-value">{healthStore.retryQueue.pending_count}</span>
			</div>
			<Badge
				label={healthStore.retryQueue.is_empty ? 'Empty' : 'Backlog'}
				variant={healthStore.retryQueue.is_empty ? 'success' : 'warning'}
			/>
		</div>
		<div class="retry-actions">
			<Button
				variant="secondary"
				onclick={() => runAction(() => healthStore.processRetryQueue())}
				disabled={actionBusy || healthStore.retryQueue?.is_empty}
			>
				{actionBusy ? 'Working...' : 'Process Queue'}
			</Button>
			<Button
				variant="danger"
				onclick={() => (clearConfirmOpen = true)}
				disabled={actionBusy || healthStore.retryQueue?.is_empty}
			>
				Clear Queue
			</Button>
		</div>
	{:else}
		<LoadingState message="Retry queue status not loaded" />
	{/if}
</Card>

{#if clearConfirmOpen}
	<Modal title="Clear Retry Queue" onClose={() => (clearConfirmOpen = false)}>
		<p>
			All pending retry operations will be discarded. Data they were meant to
			write will not be retried.
		</p>
		{#snippet actions()}
			<Button variant="secondary" onclick={() => (clearConfirmOpen = false)}>Cancel</Button>
			<Button
				variant="danger"
				onclick={() => runAction(() => healthStore.clearRetryQueue())}
				disabled={actionBusy}
			>
				{actionBusy ? 'Clearing...' : 'Clear Queue'}
			</Button>
		{/snippet}
	</Modal>
{/if}

<style>
	.retry-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}

	.retry-stat {
		display: flex;
		align-items: baseline;
		gap: 1rem;
	}

	.retry-actions {
		display: flex;
		gap: 0.75rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}





</style>
