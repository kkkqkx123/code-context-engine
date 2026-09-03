<script lang="ts">
	import { toasts, toastActions } from '$lib/stores/toast';

	function getIcon(type: string): string {
		switch (type) {
			case 'success': return '✓';
			case 'error': return '✕';
			case 'warning': return '⚠';
			default: return 'ℹ';
		}
	}
</script>

<div class="toast-container" aria-live="polite" aria-atomic="true">
	{#each $toasts as toast (toast.id)}
		<div class="toast toast-{toast.type}" role="alert">
			<span class="toast-icon" aria-hidden="true">{getIcon(toast.type)}</span>
			<span class="toast-message">{toast.message}</span>
			<button
				class="toast-dismiss"
				onclick={() => toastActions.dismiss(toast.id)}
				aria-label="Dismiss notification"
			>
				×
			</button>
		</div>
	{/each}
</div>

<style>
	.toast-container {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 10000;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-width: 400px;
		width: calc(100% - 2rem);
		pointer-events: none;
	}

	.toast {
		background: var(--black);
		color: var(--white);
		padding: 1rem;
		border-left: 4px solid var(--info);
		display: flex;
		align-items: center;
		gap: 0.75rem;
		animation: slideIn 0.3s ease;
		pointer-events: auto;
		box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
		min-height: 44px;
	}

	.toast-success {
		border-left-color: var(--success);
	}

	.toast-error {
		border-left-color: var(--danger);
	}

	.toast-warning {
		border-left-color: var(--warning);
	}

	.toast-info {
		border-left-color: var(--info);
	}

	.toast-icon {
		font-size: 1.25rem;
		font-weight: bold;
		flex-shrink: 0;
	}

	.toast-message {
		flex: 1;
		font-size: 0.9rem;
		line-height: 1.5;
		word-wrap: break-word;
	}

	.toast-dismiss {
		background: none;
		border: none;
		color: var(--white);
		font-size: 1.5rem;
		cursor: pointer;
		padding: 0;
		width: 44px;
		height: 44px;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: opacity 0.2s;
		flex-shrink: 0;
	}

	.toast-dismiss:hover {
		opacity: 0.7;
	}

	@keyframes slideIn {
		from {
			transform: translateX(100%);
			opacity: 0;
		}
		to {
			transform: translateX(0);
			opacity: 1;
		}
	}

	@media (max-width: 768px) {
		.toast-container {
			top: auto;
			bottom: 1rem;
			left: 1rem;
			right: 1rem;
			max-width: none;
			width: calc(100% - 2rem);
		}

		.toast {
			animation: slideUp 0.3s ease;
		}

		@keyframes slideUp {
			from {
				transform: translateY(100%);
				opacity: 0;
			}
			to {
				transform: translateY(0);
				opacity: 1;
			}
		}
	}
</style>
