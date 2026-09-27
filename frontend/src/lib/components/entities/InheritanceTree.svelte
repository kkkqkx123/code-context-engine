<script lang="ts">
	import type {
		ClassInheritanceResponse,
		ClassImplementationsResponse
	} from '$lib/api/entities';
	import EntityGraphView from '$lib/components/graph/EntityGraphView.svelte';
	import {
		inheritanceFocusId,
		inheritanceToElements
	} from '$lib/utils/entity-graph';

	interface Props {
		inheritance?: ClassInheritanceResponse | null;
		implementations?: ClassImplementationsResponse | null;
		focusId?: string | null;
		fallbackName?: string | null;
		onNavigate?: (id: string) => void;
	}

	let {
		inheritance = null,
		implementations = null,
		focusId = null,
		fallbackName = null,
		onNavigate = () => {}
	}: Props = $props();

	let elements = $derived(
		inheritanceToElements({ inheritance, implementations, fallbackId: focusId, fallbackName })
	);
	let effectiveFocusId = $derived(
		focusId ?? inheritanceFocusId({ inheritance, implementations, fallbackId: focusId })
	);
	let viewMode = $state<'graph' | 'list'>('graph');

	let hasData = $derived(
		(inheritance?.base_classes?.length ?? 0) > 0 ||
			(inheritance?.derived_classes?.length ?? 0) > 0 ||
			(implementations?.implemented_interfaces?.length ?? 0) > 0 ||
			(implementations?.implementing_classes?.length ?? 0) > 0
	);

	function handleItemKeydown(event: KeyboardEvent, id: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onNavigate(id);
		}
	}
</script>

<div class="inheritance-tree">
	{#if !hasData}
		<div class="empty-state">
			<p>No inheritance data available</p>
		</div>
	{:else}
		<div class="view-toggle" role="tablist" aria-label="Inheritance view mode">
			<button
				type="button"
				class="toggle-btn"
				class:active={viewMode === 'graph'}
				onclick={() => (viewMode = 'graph')}
			>
				Graph
			</button>
			<button
				type="button"
				class="toggle-btn"
				class:active={viewMode === 'list'}
				onclick={() => (viewMode = 'list')}
			>
				List
			</button>
		</div>

		{#if viewMode === 'graph'}
			<EntityGraphView {elements} focusId={effectiveFocusId} onNavigate={onNavigate} />
		{:else}
			{#if inheritance?.base_classes && inheritance.base_classes.length > 0}
				<div class="tree-section">
					<h3 class="section-title">Base Classes</h3>
					<div class="tree-list">
						{#each inheritance.base_classes as baseClass}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<div
								class="tree-item"
								tabindex="0"
								role="button"
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

			{#if inheritance?.derived_classes && inheritance.derived_classes.length > 0}
				<div class="tree-section">
					<h3 class="section-title">Derived Classes</h3>
					<div class="tree-list">
						{#each inheritance.derived_classes as derivedClass}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<div
								class="tree-item"
								tabindex="0"
								role="button"
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

			{#if implementations?.implemented_interfaces && implementations.implemented_interfaces.length > 0}
				<div class="tree-section">
					<h3 class="section-title">Implemented Interfaces</h3>
					<div class="tree-list">
						{#each implementations.implemented_interfaces as iface}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<div
								class="tree-item implementation"
								tabindex="0"
								role="button"
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

			{#if implementations?.implementing_classes && implementations.implementing_classes.length > 0}
				<div class="tree-section">
					<h3 class="section-title">Implementing Classes</h3>
					<div class="tree-list">
						{#each implementations.implementing_classes as impl}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<div
								class="tree-item implementation"
								tabindex="0"
								role="button"
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
	{/if}
</div>

<style>
	.inheritance-tree {
		border: 1px solid var(--gray-200);
		padding: 1rem;
	}

	.empty-state {
		padding: 3rem;
		text-align: center;
		color: var(--gray-400);
		font-style: italic;
	}

	.view-toggle {
		display: flex;
		gap: 0;
		margin-bottom: 1rem;
	}

	.toggle-btn {
		padding: 0.4rem 0.85rem;
		background: var(--white);
		border: 1px solid var(--gray-300);
		border-right: none;
		cursor: pointer;
		font-family: 'Space Mono', monospace;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--gray-600);
	}

	.toggle-btn:last-child {
		border-right: 1px solid var(--gray-300);
	}

	.toggle-btn.active {
		background: var(--black);
		color: var(--white);
		border-color: var(--black);
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
</style>
