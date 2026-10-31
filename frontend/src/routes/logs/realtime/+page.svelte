<script lang="ts">
	import RealtimeLogViewer from '$lib/components/logs/RealtimeLogViewer.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
</script>

<svelte:head><title>Realtime Logs</title></svelte:head>
<div class="page">
	<PageHeader title="Realtime Logs" subtitle="Live application log stream" />
	<RealtimeLogViewer />
</div>
