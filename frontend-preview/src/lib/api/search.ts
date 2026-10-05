/**
 * Search API
 * Handles code search operations with multiple query types.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type QueryType = 'vector' | 'bm25' | 'hybrid' | 'summary';

export type SearchRequest = components['schemas']['SearchRequest'];
export type SubQuery = components['schemas']['SubQueryRequest'];
export type AggregatedSearchRequest =
	components['schemas']['AggregatedSearchRequest'];
export type CallChainNode = components['schemas']['CallChainNode'];
export type SearchResultItem = components['schemas']['SearchResultItem'];
export type SearchResponse = components['schemas']['SearchResponse'];
export type EntitySearchResultItem =
	components['schemas']['EntitySearchResult'];
export type EntitySearchRequest = components['schemas']['EntitySearchRequest'];
export type EntitySearchResponse =
	components['schemas']['EntitySearchResponse'];

export const searchApi = {
	search: (request: SearchRequest): Promise<SearchResponse> =>
		call(client.POST('/api/search', { body: request })),

	aggregatedSearch: (
		request: AggregatedSearchRequest,
	): Promise<SearchResponse> =>
		call(client.POST('/api/search/aggregated', { body: request })),

	entitySearch: (request: EntitySearchRequest): Promise<EntitySearchResponse> =>
		call(client.POST('/api/entities/search', { body: request })),
};
