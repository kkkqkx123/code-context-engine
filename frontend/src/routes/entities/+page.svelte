<script lang="ts">
	/**
	 * The entity list is a view of the query workbench now. This route stays
	 * reachable for old bookmarks and forwards its query string unchanged.
	 */
	import { onMount } from 'svelte';
	import { SvelteURLSearchParams } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';

	onMount(() => {
		const params = new SvelteURLSearchParams(window.location.search);
		params.set('view', 'entities');
		void goto(resolve(`/search?${params}`), { replaceState: true });
	});
</script>

<svelte:head>
	<title>Entity Search - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<LoadingState message="Opening entity search in the query workbench..." />
	</div>
</div>
