<script lang="ts" generics="P extends Record<string, unknown>">
	/**
	 * On-demand panel wrapper.
	 *
	 * Owns only load orchestration: the loader runs once when the sentinel first
	 * becomes visible, while loading a loading state is shown, and a failed
	 * import shows an inline error with a retry action. The loaded component is
	 * rendered with the supplied one-way props; no page logic lives here.
	 */
	import { onMount, type Component } from 'svelte';
	import LoadingState from './LoadingState.svelte';
	import InlineError from './InlineError.svelte';
	import Button from './Button.svelte';

	interface LazyPanelProps {
		loader: () => Promise<{ default: Component<P> }>;
		componentProps?: P;
		loadingMessage?: string;
		errorMessage?: string;
	}

	let {
		loader,
		componentProps = {} as P,
		loadingMessage = 'Loading panel…',
		errorMessage = 'Failed to load this panel',
	}: LazyPanelProps = $props();

	let Loaded = $state<Component<P> | null>(null);
	let loading = $state(false);
	let failed = $state(false);
	let sentinel: HTMLDivElement | undefined = $state(undefined);
	let disposed = false;

	async function load() {
		if (loading || Loaded) return;
		loading = true;
		failed = false;
		try {
			const module = await loader();
			if (!disposed) Loaded = module.default;
		} catch (error) {
			console.error('Lazy panel failed to load:', error);
			if (!disposed) failed = true;
		} finally {
			if (!disposed) loading = false;
		}
	}

	onMount(() => {
		disposed = false;
		if (typeof IntersectionObserver === 'undefined' || !sentinel) {
			void load();
			return;
		}
		const observer = new IntersectionObserver((entries) => {
			if (entries.some((entry) => entry.isIntersecting)) {
				observer.disconnect();
				void load();
			}
		});
		observer.observe(sentinel);
		return () => {
			disposed = true;
			observer.disconnect();
		};
	});
</script>

<div class="lazy-panel">
	{#if Loaded}
		<Loaded {...componentProps} />
	{:else if failed}
		<div class="lazy-fallback">
			<InlineError message={errorMessage} />
			<Button variant="secondary" size="xs" onclick={load}>Retry</Button>
		</div>
	{:else}
		<div class="lazy-sentinel" bind:this={sentinel}></div>
		{#if loading}
			<LoadingState message={loadingMessage} />
		{/if}
	{/if}
</div>

<style>
	.lazy-panel {
		min-width: 0;
	}

	.lazy-sentinel {
		height: 1px;
	}

	.lazy-fallback {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		align-items: flex-start;
	}
</style>
