/**
 * Search State Store
 * Manages search queries and results
 */

import { writable, get } from 'svelte/store';
import { searchApi, type SearchRequest, type SearchResultItem, type QueryType } from '../api/search';
import { currentProjectId } from './project';

export interface SearchState {
	query: string;
	projectId: number;
	queryType: QueryType;
	results: SearchResultItem[];
	total: number;
	isSearching: boolean;
	filters: {
		directory_prefix: string;
		min_score: number;
	};
	pagination: {
		page: number;
		limit: number;
	};
}

export const searchState = writable<SearchState>({
	query: '',
	projectId: get(currentProjectId),
	queryType: 'hybrid',
	results: [],
	total: 0,
	isSearching: false,
	filters: {
		directory_prefix: '',
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
			const projectId = get(currentProjectId);
			const request: SearchRequest = {
				project_id: projectId,
				query: state!.query,
				query_type: state!.queryType,
				limit: state!.pagination.limit,
				min_score: state!.filters.min_score || undefined,
				directory_prefix: state!.filters.directory_prefix || undefined,
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