<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Button from '$lib/components/ui/Button.svelte';
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
				<span class="kv-label">Pending operations</span>
				<span class="retry-count">{healthStore.retryQueue.pending_count}</span>
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
	<div
		class="dialog-overlay"
		onclick={() => (clearConfirmOpen = false)}
		onkeydown={(e) => {
			if (e.key === 'Escape') clearConfirmOpen = false;
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
			aria-labelledby="retry-dialog-title"
			tabindex="-1"
			onkeydown={(e) => {
				if (e.key === 'Escape') clearConfirmOpen = false;
			}}
		>
			<h2 id="retry-dialog-title">Clear Retry Queue</h2>
			<p class="dialog-warning">
				All pending retry operations will be discarded. Data they were meant to
				write will not be retried.
			</p>
			<div class="dialog-actions">
				<Button variant="secondary" onclick={() => (clearConfirmOpen = false)}
					>Cancel</Button
				>
				<Button
					variant="danger"
					onclick={() => runAction(() => healthStore.clearRetryQueue())}
					disabled={actionBusy}
				>
					{actionBusy ? 'Clearing...' : 'Clear Queue'}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.kv-label {
		font-family: 'Space Mono', monospace;
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--gray-600);
	}

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

	.retry-count {
		font-size: 1.75rem;
		font-weight: 700;
		letter-spacing: -0.03em;
	}

	.retry-actions {
		display: flex;
		gap: 0.75rem;
		margin-top: 1rem;
		padding-top: 1rem;
		border-top: 1px solid var(--gray-200);
	}

	.dialog-overlay {
		position: fixed;
		inset: 0;
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
		max-width: 480px;
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
</style>
