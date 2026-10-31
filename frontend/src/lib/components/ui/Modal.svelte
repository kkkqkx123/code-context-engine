<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		onClose: () => void;
		children?: Snippet;
		actions?: Snippet;
	}

	let { title, onClose, children, actions }: Props = $props();

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') onClose();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="modal-backdrop" role="presentation" onclick={() => onClose()}>
	<div
		class="modal"
		role="dialog"
		aria-modal="true"
		aria-label={title}
		onclick={(e) => e.stopPropagation()}
		onkeydown={(e) => {
			if (e.key === 'Enter' && e.target === e.currentTarget)
				e.stopPropagation();
		}}
		tabindex="-1"
	>
		<h3 class="modal-title">{title}</h3>
		<div class="modal-body">
			{@render children?.()}
		</div>
		{#if actions}
			<div class="modal-actions">
				{@render actions()}
			</div>
		{/if}
	</div>
</div>

<style>
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
		background: var(--white);
		border: 1px solid var(--black);
		padding: 1.25rem 1.5rem;
		max-width: 460px;
		width: 90%;
	}

	.modal-title {
		margin: 0 0 0.5rem;
		font-size: 1.1rem;
	}

	.modal-body {
		font-size: 0.9rem;
		color: var(--gray-600);
		line-height: 1.6;
	}

	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 1rem;
	}
</style>
