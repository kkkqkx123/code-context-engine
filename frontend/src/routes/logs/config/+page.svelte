<script lang="ts">
	import LogConfigPanel from '$lib/components/logs/LogConfigPanel.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
</script>

<svelte:head><title>Log Config</title></svelte:head>
<div class="page">
	<PageHeader title="Log Config" subtitle="Configure logging" />
	<LogConfigPanel />
</div>
