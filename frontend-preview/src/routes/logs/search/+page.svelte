<script lang="ts">
	import LogSearchPanel from '$lib/components/logs/LogSearchPanel.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
</script>
<svelte:head><title>Log Search</title></svelte:head>
<div class="page">
	<PageHeader title="Log Search" subtitle="Search historical logs" />
	<LogSearchPanel />
</div>
