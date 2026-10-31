<script lang="ts">
	import { searchStore } from '$lib/stores/search.svelte';
	import Button from '../ui/Button.svelte';

	interface Props {
		onSearch?: () => void;
	}

	let { onSearch = () => {} }: Props = $props();

	let query = $state('');

	function submit() {
		const trimmed = query.trim();
		if (!trimmed) return;
		searchStore.setQuery(trimmed);
		onSearch();
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') submit();
	}
</script>

<div class="search-container">
	<input
		type="text"
		class="search-hero"
		placeholder="Search codebase... (e.g., 'authentication function', 'database connection')"
		bind:value={query}
		onkeydown={handleKeydown}
	/>
	<Button variant="primary" onclick={submit} disabled={!query.trim()}>
		Search
	</Button>
</div>

<style>
	.search-container {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		margin-bottom: 1.5rem;
	}
</style>
