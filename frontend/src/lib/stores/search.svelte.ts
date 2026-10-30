/**
 * Search State Store.
 * Single entry point for both standard and aggregated search.
 *
 * Pagination model: the backend API has no offset parameter, so each fetch
 * requests `limit = page * pageSize` and slices client-side. Filters apply
 * uniformly to standard and aggregated modes; changing a filter or query
 * type after a search marks the results stale and auto re-runs the search.
 *
 * Rune-based replacement of the previous `writable<SearchState>` + `searchActions`
 * object. State is `$state` fields; actions are methods; the URL helpers and
 * `buildCommonFilterFields`/`buildAggregatedRequest` stay at module level.
 */

import { errorMessage } from '$lib/utils/errors';
import {
	searchApi,
	type SearchRequest,
	type SearchResultItem,
	type QueryType,
	type SubQuery,
} from '../api/search';
import { projectStore, onProjectChange } from './project.svelte';

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
	/** Empty = auto-detect; otherwise semantic, keyword, hybrid, entity. */
	query_intent: string;
	/** null = leave expansion at server default */
	expand: boolean | null;
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
			query_intent: '',
			expand: null,
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
		query_intent: params.get('intent') ?? '',
		expand: parseRerankParam(params.get('expand')),
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
		query_intent: state.filters.query_intent || undefined,
		expand: state.filters.expand ?? undefined,
	};
}

let requestSeq = 0;
let autoSearchTimer: ReturnType<typeof setTimeout> | undefined;

class SearchStore {
	query = $state('');
	mode = $state<SearchMode>(modeFromUrl());
	/** Aggregated-mode sub-queries; empty list = auto-decompose from the main query */
	subQueries = $state<SubQueryDraft[]>([]);
	results = $state<SearchResultItem[]>([]);
	total = $state(0);
	elapsedMs = $state<number | null>(null);
	sourcesUsed = $state<string[]>([]);
	failedSubQueries = $state<string[]>([]);
	/** Relation epoch backing the last result set, when reported. */
	relationEpoch = $state<number | null>(null);
	/** True when the serving relation snapshot reported itself stale. */
	relationStale = $state(false);
	isSearching = $state(false);
	error = $state<string | null>(null);
	/** True when filters/mode changed after the last search */
	stale = $state(false);
	filters = $state<SearchFilters>(filtersFromUrl());
	pagination = $state({
		page: 1,
		pageSize: 10,
	});

	setQuery(query: string) {
		this.query = query;
	}

	setMode(mode: SearchMode) {
		this.mode = mode;
		this.stale = true;
		this.scheduleAutoSearch();
	}

	setAggSubQueries(subQueries: SubQueryDraft[]) {
		this.subQueries = subQueries.slice(0, MAX_SUB_QUERIES);
		this.stale = true;
		this.scheduleAutoSearch();
	}

	updateFilter<K extends keyof SearchState['filters']>(
		key: K,
		value: SearchState['filters'][K],
	) {
		this.filters = { ...this.filters, [key]: value };
		this.stale = true;
		this.scheduleAutoSearch();
	}

	/** Auto re-run the last search after filters/mode changed (debounced). */
	scheduleAutoSearch() {
		if (!this.query.trim() || !this.stale) return;
		clearTimeout(autoSearchTimer);
		autoSearchTimer = setTimeout(() => {
			if (this.stale) void this.executeSearch();
		}, 400);
	}

	/** Drop the result set and re-run it under the newly selected project. */
	markProjectChanged() {
		this.results = [];
		this.total = 0;
		this.elapsedMs = null;
		this.sourcesUsed = [];
		this.failedSubQueries = [];
		this.relationEpoch = null;
		this.relationStale = false;
		this.stale = true;
		this.scheduleAutoSearch();
	}

	async executeSearch(page = 1) {
		this.isSearching = true;
		this.error = null;

		const query = this.query.trim();
		if (!query) return;

		// Race guard: ignore responses from outdated requests.
		const seq = ++requestSeq;

		const projectId = projectStore.currentProjectId;
		const filterFields = buildCommonFilterFields(this);
		// Cumulative fetch: page N needs page*pageSize rows on the client.
		const limit = page * this.pagination.pageSize;

		try {
			const response =
				this.mode === AGGREGATED_TYPE
					? await buildAggregatedRequest(
							projectId,
							query,
							this,
							limit,
							filterFields,
						)
					: await searchApi.search({
							project_id: projectId,
							query,
							query_type: this.mode,
							limit,
							...filterFields,
						} satisfies SearchRequest);

			if (seq !== requestSeq) return;

			this.results = response.items;
			this.total = response.total;
			this.elapsedMs = response.elapsed_ms;
			this.sourcesUsed = response.sources_used ?? [];
			this.failedSubQueries = response.failed_sub_queries ?? [];
			this.relationEpoch = response.relation_epoch ?? null;
			this.relationStale = response.relation_stale ?? false;
			this.isSearching = false;
			this.error = null;
			this.stale = false;
			this.pagination = { ...this.pagination, page };
			this.#syncUrl(query);
		} catch (error) {
			if (seq !== requestSeq) return;
			const message = errorMessage(error) ?? 'Search failed';
			this.isSearching = false;
			this.error = message;
		}
	}

	setPage(page: number) {
		this.pagination = { ...this.pagination, page };
	}

	// Paginated view of the cumulatively fetched results.
	getPaginatedResults(): SearchResultItem[] {
		const start = (this.pagination.page - 1) * this.pagination.pageSize;
		const end = start + this.pagination.pageSize;
		return this.results.slice(start, end);
	}

	/** Total number of client-side pages given the fetched result set. */
	totalPages(): number {
		return Math.max(1, Math.ceil(this.results.length / this.pagination.pageSize));
	}

	#syncUrl(currentQuery: string) {
		if (typeof window === 'undefined') return;
		const params = new URLSearchParams();
		params.set('q', currentQuery);
		params.set('mode', this.mode);
		if (this.filters.directory_prefix)
			params.set('dir', this.filters.directory_prefix);
		if (this.filters.min_score > 0)
			params.set('min', String(this.filters.min_score));
		if (this.filters.exclude_content_types)
			params.set('excl_types', this.filters.exclude_content_types);
		if (this.filters.exclude_categories)
			params.set('excl_cats', this.filters.exclude_categories);
		if (this.filters.include_categories)
			params.set('incl_cats', this.filters.include_categories);
		if (this.filters.exclude_patterns)
			params.set('excl_glob', this.filters.exclude_patterns);
		if (this.filters.include_patterns)
			params.set('incl_glob', this.filters.include_patterns);
		if (this.filters.enable_rerank !== null)
			params.set('rerank', this.filters.enable_rerank ? 'on' : 'off');
		if (this.filters.rerank_max_candidates !== null)
			params.set('rerank_n', String(this.filters.rerank_max_candidates));
		if (this.filters.query_intent)
			params.set('intent', this.filters.query_intent);
		if (this.filters.expand !== null)
			params.set('expand', this.filters.expand ? 'on' : 'off');
		window.history.replaceState(
			null,
			'',
			`${window.location.pathname}?${params}`,
		);
	}
}

export const searchStore = new SearchStore();

// Results belong to the project that was selected when they were fetched.
onProjectChange(() => {
	searchStore.markProjectChanged();
});

async function buildAggregatedRequest(
	projectId: number | null,
	query: string,
	state: SearchStore,
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
