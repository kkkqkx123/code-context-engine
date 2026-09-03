/**
 * Search State Store
 * Manages search queries and results
 */

import { writable } from 'svelte/store';
import { searchApi, type SearchRequest, type SearchResultItem, type QueryType } from '../api/search';

export interface SearchState {
	query: string;
	projectId: number;
	queryType: QueryType;
	results: SearchResultItem[];
	total: number;
	isSearching: boolean;
	filters: {
		file_extensions: string[];
		directory_prefix: string;
		entity_types: string[];
		languages: string[];
		min_score: number;
	};
	pagination: {
		page: number;
		limit: number;
	};
}

export const searchState = writable<SearchState>({
	query: '',
	projectId: 1,
	queryType: 'hybrid',
	results: [],
	total: 0,
	isSearching: false,
	filters: {
		file_extensions: [],
		directory_prefix: '',
		entity_types: [],
		languages: [],
		min_score: 0,
	},
	pagination: {
		page: 1,
		limit: 10,
	},
});

// Actions
export const searchActions = {
	setQuery(query: string) {
		searchState.update(state => ({ ...state, query }));
	},

	setQueryType(type: QueryType) {
		searchState.update(state => ({ ...state, queryType: type }));
	},

	updateFilter<K extends keyof SearchState['filters']>(
		key: K,
		value: SearchState['filters'][K]
	) {
		searchState.update(state => ({
			...state,
			filters: { ...state.filters, [key]: value },
		}));
	},

	async executeSearch() {
		let state: SearchState;
		searchState.subscribe(s => { state = s; })();

		if (!state!.query.trim()) return;

		searchState.update(s => ({ ...s, isSearching: true }));

		try {
			const request: SearchRequest = {
				project_id: state!.projectId,
				query: state!.query,
				query_type: state!.queryType,
				limit: state!.pagination.limit,
				min_score: state!.filters.min_score || undefined,
				file_extensions: state!.filters.file_extensions.length > 0 ? state!.filters.file_extensions : undefined,
				directory_prefix: state!.filters.directory_prefix || undefined,
				entity_types: state!.filters.entity_types.length > 0 ? state!.filters.entity_types : undefined,
				languages: state!.filters.languages.length > 0 ? state!.filters.languages : undefined,
			};

			const response = await searchApi.search(request);

			searchState.update(s => ({
				...s,
				results: response.items,
				total: response.total,
				isSearching: false,
			}));
		} catch (error) {
			console.error('Search failed:', error);
			searchState.update(s => ({ ...s, isSearching: false }));
		}
	},
	setPage(page: number) {
		searchState.update(state => ({
			...state,
			pagination: { ...state.pagination, page },
		}));
	},

	// Get paginated results (client-side pagination)
	getPaginatedResults(): SearchResultItem[] {
		let currentState: SearchState | undefined;
		searchState.subscribe(s => { currentState = s; })();
		
		if (!currentState) return [];
		
		const start = (currentState.pagination.page - 1) * currentState.pagination.limit;
		const end = start + currentState.pagination.limit;
		return currentState.results.slice(start, end);
	},
};