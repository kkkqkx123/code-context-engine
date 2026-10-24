<script lang="ts">
	import LogFileManager from '$lib/components/logs/LogFileManager.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
</script>
<svelte:head><title>Log Files</title></svelte:head>
<div class="page">
	<PageHeader title="Log Files" subtitle="Manage log files" />
	<LogFileManager />
</div>
