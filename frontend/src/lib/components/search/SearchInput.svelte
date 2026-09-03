<script lang="ts">
	import { searchState, searchActions } from '$lib/stores/search';
	import Button from '../ui/Button.svelte';

	interface Props {
		onSearch?: () => void;
	}

	let { onSearch = () => {} }: Props = $props();

	let query = $state('');

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			searchActions.setQuery(query);
			onSearch();
		}
	}

	function handleSearch() {
		searchActions.setQuery(query);
		onSearch();
	}
</script>

<div class="search-container">
	<input
		type="text"
		class="search-input"
		placeholder="Search codebase... (e.g., 'authentication function', 'database connection')"
		bind:value={query}
		onkeydown={handleKeydown}
	/>
	<Button variant="primary" onclick={handleSearch}>
		Search
	</Button>
</div>

<style>
	.search-container {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 1rem;
		margin-bottom: 2rem;
	}

	.search-input {
		font-family: 'Space Grotesk', sans-serif;
		font-size: 1.25rem;
		padding: 1rem 1.5rem;
		border: 1px solid var(--black);
		background: var(--white);
		color: var(--black);
		outline: none;
		transition: border-color 0.2s;
	}

	.search-input:focus {
		border-color: var(--accent);
	}

	.search-input::placeholder {
		color: var(--gray-400);
	}
</style>
