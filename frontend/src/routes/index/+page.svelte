<script lang="ts">
	/**
	 * Index controls moved into the project workbench. This route stays
	 * reachable for old bookmarks and forwards to the merged entry point.
	 */
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';

	onMount(() => {
		void goto(resolve('/projects'), { replaceState: true });
	});
</script>

<svelte:head>
	<title>Project Workbench - Code Context Engine</title>
</svelte:head>

<div class="page">
	<div class="container">
		<LoadingState message="Opening the project workbench..." />
	</div>
</div>
