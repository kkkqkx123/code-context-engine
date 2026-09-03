<script lang="ts">
	import type { ClassInheritanceResponse, ClassImplementationsResponse } from '$lib/api/entities';

	interface Props {
		inheritance?: ClassInheritanceResponse | null;
		implementations?: ClassImplementationsResponse | null;
		onNavigate?: (id: string) => void;
	}

	let {
		inheritance = null,
		implementations = null,
		onNavigate = () => {}
	}: Props = $props();

	function handleItemKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onNavigate(id);
		}
	}
</script>

<div class="inheritance-tree">
	{#if !inheritance || (!inheritance.base_classes?.length && !inheritance.derived_classes?.length)}
		<div class="empty-state">
			<p>No inheritance data available</p>
		</div>
	{:else}
		{#if inheritance.base_classes && inheritance.base_classes.length > 0}
			<div class="tree-section">
				<h3 class="section-title">Base Classes</h3>
				<div class="tree-list">
					{#each inheritance.base_classes as baseClass, i}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="tree-item"
							tabindex="0"
							role="button"
							style="margin-left: 0px; opacity: 0; animation: popIn 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {i * 0.1}s;"
							onclick={() => onNavigate(baseClass.class_id)}
							onkeydown={(e) => handleItemKeydown(e, baseClass.class_id)}
						>
							<span class="item-icon">▲</span>
							<span class="item-name">{baseClass.class_name}</span>
							<span class="item-location">{baseClass.file_path.split('/').pop()}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if inheritance.derived_classes && inheritance.derived_classes.length > 0}
			<div class="tree-section">
				<h3 class="section-title">Derived Classes</h3>
				<div class="tree-list">
					{#each inheritance.derived_classes as derivedClass, i}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="tree-item"
							tabindex="0"
							role="button"
							style="margin-left: 0px; opacity: 0; animation: popIn 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {(inheritance.base_classes?.length || 0 + i) * 0.1}s;"
							onclick={() => onNavigate(derivedClass.class_id)}
							onkeydown={(e) => handleItemKeydown(e, derivedClass.class_id)}
						>
							<span class="item-icon">▼</span>
							<span class="item-name">{derivedClass.class_name}</span>
							<span class="item-location">{derivedClass.file_path.split('/').pop()}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if implementations && implementations.implemented_interfaces && implementations.implemented_interfaces.length > 0}
			<div class="tree-section">
				<h3 class="section-title">Implemented Interfaces</h3>
				<div class="tree-list">
					{#each implementations.implemented_interfaces as iface, i}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="tree-item implementation"
							tabindex="0"
							role="button"
							style="opacity: 0; animation: popIn 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {i * 0.1}s;"
							onclick={() => onNavigate(iface.interface_id)}
							onkeydown={(e) => handleItemKeydown(e, iface.interface_id)}
						>
							<span class="item-icon">◆</span>
							<span class="item-name">{iface.interface_name}</span>
							<span class="item-location">{iface.file_path.split('/').pop()}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if implementations && implementations.implementing_classes && implementations.implementing_classes.length > 0}
			<div class="tree-section">
				<h3 class="section-title">Implementing Classes</h3>
				<div class="tree-list">
					{#each implementations.implementing_classes as impl, i}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="tree-item implementation"
							tabindex="0"
							role="button"
							style="opacity: 0; animation: popIn 0.55s cubic-bezier(0.2, 0.9, 0.2, 1) forwards; animation-delay: {i * 0.1}s;"
							onclick={() => onNavigate(impl.class_id)}
							onkeydown={(e) => handleItemKeydown(e, impl.class_id)}
						>
							<span class="item-icon">◆</span>
							<span class="item-name">{impl.class_name}</span>
							<span class="item-location">{impl.file_path.split('/').pop()}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	{/if}
</div>

<style>
	.inheritance-tree {
		border: 1px solid var(--gray-200);
		padding: 2rem;
	}

	.empty-state {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}

	.tree-section {
		margin-bottom: 2rem;
	}

	.tree-section:last-child {
		margin-bottom: 0;
	}

	.section-title {
		font-family: 'Space Mono', monospace;
		text-transform: uppercase;
		font-size: 0.75rem;
		letter-spacing: 0.1em;
		margin-bottom: 1rem;
		color: var(--gray-600);
		padding-bottom: 0.5rem;
		border-bottom: 1px solid var(--gray-200);
	}

	.tree-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 0.5rem;
	}

	/* Clickable tree item: black border (per border convention) */
	.tree-item {
		padding: 0.75rem 1rem;
		border: 1px solid var(--black);
		cursor: pointer;
		transition: all 0.2s;
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 1rem;
		align-items: center;
	}

	.tree-item:hover {
		background-color: var(--gray-100);
		border-left: 3px solid var(--accent);
	}

	.tree-item.implementation:hover {
		border-left: 3px solid var(--black);
	}

	.item-icon {
		font-size: 1rem;
		color: var(--gray-600);
		width: 20px;
		text-align: center;
	}

	.item-name {
		font-family: 'Space Grotesk', sans-serif;
		font-weight: 700;
	}

	.item-location {
		font-family: 'Space Mono', monospace;
		font-size: 0.75rem;
		color: var(--gray-400);
	}

	@keyframes popIn {
		to {
			opacity: 1;
		}
	}
</style>
