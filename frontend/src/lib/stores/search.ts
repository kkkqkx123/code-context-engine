/**
 * Search State Store
 * Single entry point for both standard and aggregated search.
 *
 * Pagination model: the backend API has no offset parameter, so each fetch
 * requests `limit = page * pageSize` and slices client-side. Filters apply
 * uniformly to standard and aggregated modes; changing a filter or query
 * type after a search marks the results stale and auto re-runs the search.
 */

import { writable, get } from 'svelte/store';
import {
	searchApi,
	type SearchRequest,
	type SearchResultItem,
	type QueryType,
	type SubQuery,
} from '../api/search';
import { currentProjectId, onProjectChange } from './project';
import { errorMessage } from '$lib/utils/errors';

export const QUERY_TYPES: readonly QueryType[] = [
	'vector',
	'bm25',
	'hybrid',
	'summary',
];

/** Query types a sub-query may use (backend validates: vector, bm25, hybrid, summary) */
export const SUB_QUERY_TYPES: readonly QueryType[] = [
	'vector',
	'bm25',
	'hybrid',
	'summary',
];

/** Pseudo query type: fan out into weighted sub-queries via /aggregated */
export const AGGREGATED_TYPE = 'aggregated' as const;
export type SearchMode = QueryType | typeof AGGREGATED_TYPE;

/** Backend rejects aggregated requests with more than this many sub-queries */
export const MAX_SUB_QUERIES = 10;

export interface SubQueryDraft {
	text: string;
	query_type: QueryType;
	weight: number;
}

export interface SearchFilters {
	directory_prefix: string;
	min_score: number;
	exclude_content_types: string;
	exclude_categories: string;
	include_categories: string;
	exclude_patterns: string;
	include_patterns: string;
	/** null = leave rerank at server default */
	enable_rerank: boolean | null;
	rerank_max_candidates: number | null;
}

export interface SearchState {
	query: string;
	mode: SearchMode;
	/** Aggregated-mode sub-queries; empty list = auto-decompose from the main query */
	subQueries: SubQueryDraft[];
	results: SearchResultItem[];
	total: number;
	elapsedMs: number | null;
	sourcesUsed: string[];
	failedSubQueries: string[];
	/** Relation epoch backing the last result set, when reported. */
	relationEpoch: number | null;
	/** True when the serving relation snapshot reported itself stale. */
	relationStale: boolean;
	isSearching: boolean;
	error: string | null;
	/** True when filters/mode changed after the last search */
	stale: boolean;
	filters: SearchFilters;
	pagination: {
		page: number;
		pageSize: number;
	};
}

function parseCsv(value: string): string[] {
	return value
		.split(',')
		.map((s) => s.trim())
		.filter(Boolean);
}

/** Parse globs separated by comma or whitespace. */
function parseGlobs(value: string): string[] {
	return value
		.split(/[\s,]+/)
		.map((s) => s.trim())
		.filter(Boolean);
}

function parsePositiveInt(raw: string | null): number | null {
	if (raw === null || raw === '') return null;
	const n = Number(raw);
	return Number.isInteger(n) && n > 0 ? n : null;
}

function parseRerankParam(raw: string | null): boolean | null {
	if (raw === null || raw === '') return null;
	if (raw === 'on' || raw === 'true' || raw === '1') return true;
	if (raw === 'off' || raw === 'false' || raw === '0') return false;
	return null;
}

function filtersFromUrl(): SearchFilters {
	if (typeof window === 'undefined') {
		return {
			directory_prefix: '',
			min_score: 0,
			exclude_content_types: '',
			exclude_categories: '',
			include_categories: '',
			exclude_patterns: '',
			include_patterns: '',
			enable_rerank: null,
			rerank_max_candidates: null,
		};
	}
	const params = new URLSearchParams(window.location.search);
	return {
		directory_prefix: params.get('dir') ?? '',
		min_score: Number(params.get('min') ?? 0) || 0,
		exclude_content_types: params.get('excl_types') ?? '',
		exclude_categories: params.get('excl_cats') ?? '',
		include_categories: params.get('incl_cats') ?? '',
		exclude_patterns: params.get('excl_glob') ?? '',
		include_patterns: params.get('incl_glob') ?? '',
		enable_rerank: parseRerankParam(params.get('rerank')),
		rerank_max_candidates: parsePositiveInt(params.get('rerank_n')),
	};
}

function modeFromUrl(): SearchMode {
	if (typeof window === 'undefined') return AGGREGATED_TYPE;
	const mode = new URLSearchParams(window.location.search).get('mode');
	if (mode === AGGREGATED_TYPE) return AGGREGATED_TYPE;
	return (QUERY_TYPES as readonly string[]).includes(mode ?? '')
		? (mode as QueryType)
		: AGGREGATED_TYPE;
}

export const searchState = writable<SearchState>({
	query: '',
	mode: modeFromUrl(),
	subQueries: [],
	results: [],
	total: 0,
	elapsedMs: null,
	sourcesUsed: [],
	failedSubQueries: [],
	relationEpoch: null,
	relationStale: false,
	isSearching: false,
	error: null,
	stale: false,
	filters: filtersFromUrl(),
	pagination: {
		page: 1,
		pageSize: 10,
	},
});

let requestSeq = 0;
let autoSearchTimer: ReturnType<typeof setTimeout> | undefined;

function syncUrl(state: SearchState) {
	if (typeof window === 'undefined') return;
	const params = new URLSearchParams();
	params.set('q', state.query);
	params.set('mode', state.mode);
	if (state.filters.directory_prefix)
		params.set('dir', state.filters.directory_prefix);
	if (state.filters.min_score > 0)
		params.set('min', String(state.filters.min_score));
	if (state.filters.exclude_content_types)
		params.set('excl_types', state.filters.exclude_content_types);
	if (state.filters.exclude_categories)
		params.set('excl_cats', state.filters.exclude_categories);
	if (state.filters.include_categories)
		params.set('incl_cats', state.filters.include_categories);
	if (state.filters.exclude_patterns)
		params.set('excl_glob', state.filters.exclude_patterns);
	if (state.filters.include_patterns)
		params.set('incl_glob', state.filters.include_patterns);
	if (state.filters.enable_rerank !== null)
		params.set('rerank', state.filters.enable_rerank ? 'on' : 'off');
	if (state.filters.rerank_max_candidates !== null)
		params.set('rerank_n', String(state.filters.rerank_max_candidates));
	window.history.replaceState(
		null,
		'',
		`${window.location.pathname}?${params}`,
	);
}

function buildCommonFilterFields(state: SearchState) {
	return {
		min_score: state.filters.min_score || undefined,
		directory_prefix: state.filters.directory_prefix || undefined,
		exclude_content_types: parseCsv(state.filters.exclude_content_types),
		exclude_categories: parseCsv(state.filters.exclude_categories),
		include_categories: parseCsv(state.filters.include_categories),
		exclude_patterns: parseGlobs(state.filters.exclude_patterns),
		include_patterns: parseGlobs(state.filters.include_patterns),
		enable_rerank: state.filters.enable_rerank ?? undefined,
		rerank_max_candidates: state.filters.rerank_max_candidates ?? undefined,
	};
}

/** Actions */
export const searchActions = {
	setQuery(query: string) {
		searchState.update((state) => ({ ...state, query }));
	},

	setMode(mode: SearchMode) {
		searchState.update((state) => ({ ...state, mode, stale: true }));
		this.scheduleAutoSearch();
	},

	setAggSubQueries(subQueries: SubQueryDraft[]) {
		searchState.update((s) => ({
			...s,
			subQueries: subQueries.slice(0, MAX_SUB_QUERIES),
			stale: true,
		}));
		this.scheduleAutoSearch();
	},

	updateFilter<K extends keyof SearchState['filters']>(
		key: K,
		value: SearchState['filters'][K],
	) {
		searchState.update((state) => ({
			...state,
			filters: { ...state.filters, [key]: value },
			stale: true,
		}));
		this.scheduleAutoSearch();
	},

	/** Auto re-run the last search after filters/mode changed (debounced). */
	scheduleAutoSearch() {
		const state = get(searchState);
		if (!state.query.trim() || !state.stale) return;
		clearTimeout(autoSearchTimer);
		autoSearchTimer = setTimeout(() => {
			if (get(searchState).stale) void this.executeSearch();
		}, 400);
	},

	/** Drop the result set and re-run it under the newly selected project. */
	markProjectChanged() {
		searchState.update((state) => ({
			...state,
			results: [],
			total: 0,
			elapsedMs: null,
			sourcesUsed: [],
			failedSubQueries: [],
			relationEpoch: null,
			relationStale: false,
			stale: true,
		}));
		this.scheduleAutoSearch();
	},

	async executeSearch(page = 1) {
		let state: SearchState;
		searchState.subscribe((s) => {
			state = s;
		})();

		const query = state!.query.trim();
		if (!query) return;

		// Race guard: ignore responses from outdated requests.
		const seq = ++requestSeq;

		searchState.update((s) => ({ ...s, isSearching: true, error: null }));

		try {
			const projectId = get(currentProjectId);
			const filterFields = buildCommonFilterFields(state!);
			// Cumulative fetch: page N needs page*pageSize rows on the client.
			const limit = page * state!.pagination.pageSize;

			const response =
				state!.mode === AGGREGATED_TYPE
					? await buildAggregatedRequest(
							projectId,
							query,
							state!,
							limit,
							filterFields,
						)
					: await searchApi.search({
							project_id: projectId,
							query,
							query_type: state!.mode,
							limit,
							...filterFields,
						} satisfies SearchRequest);

			if (seq !== requestSeq) return;

			searchState.update((s) => ({
				...s,
				results: response.items,
				total: response.total,
				elapsedMs: response.elapsed_ms,
				sourcesUsed: response.sources_used ?? [],
				failedSubQueries: response.failed_sub_queries ?? [],
				relationEpoch: response.relation_epoch ?? null,
				relationStale: response.relation_stale ?? false,
				isSearching: false,
				error: null,
				stale: false,
				pagination: { ...s.pagination, page },
			}));
			syncUrl({ ...state!, query });
		} catch (error) {
			if (seq !== requestSeq) return;
			const message = errorMessage(error) ?? 'Search failed';
			searchState.update((s) => ({
				...s,
				isSearching: false,
				error: message,
			}));
		}
	},

	setPage(page: number) {
		searchState.update((state) => ({
			...state,
			pagination: { ...state.pagination, page },
		}));
	},

	// Paginated view of the cumulatively fetched results.
	getPaginatedResults(): SearchResultItem[] {
		const currentState = get(searchState);
		const start =
			(currentState.pagination.page - 1) * currentState.pagination.pageSize;
		const end = start + currentState.pagination.pageSize;
		return currentState.results.slice(start, end);
	},

	/** Total number of client-side pages given the fetched result set. */
	totalPages(): number {
		const state = get(searchState);
		return Math.max(
			1,
			Math.ceil(state.results.length / state.pagination.pageSize),
		);
	},
};

// Results belong to the project that was selected when they were fetched.
onProjectChange(() => {
	searchActions.markProjectChanged();
});

async function buildAggregatedRequest(
	projectId: number | null,
	query: string,
	state: SearchState,
	limit: number,
	filterFields: ReturnType<typeof buildCommonFilterFields>,
) {
	// Non-empty drafts become weighted sub-queries; an empty draft list falls
	// back to a BM25 + vector decomposition of the main query.
	const drafts = state.subQueries.filter((sq) => sq.text.trim());
	const subQueries: SubQuery[] = drafts.length
		? drafts.map((sq) => ({
				text: sq.text.trim(),
				query_type: sq.query_type,
				weight: sq.weight,
			}))
		: [
				{ text: query, query_type: 'bm25', weight: 1.2 },
				{ text: query, query_type: 'vector', weight: 1.0 },
			];
	return searchApi.aggregatedSearch({
		project_id: projectId,
		sub_queries: subQueries,
		limit,
		...filterFields,
	});
}
