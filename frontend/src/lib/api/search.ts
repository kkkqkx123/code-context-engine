/**
 * Search API
 * Handles code search operations with multiple query types
 */

import { apiClient } from './client';

export type QueryType = 'vector' | 'bm25' | 'hybrid' | 'summary' | 'hierarchical' | 'semantic_with_relations';

export interface SearchRequest {
	project_id?: number;
	project_path?: string;
	query: string;
	query_type?: QueryType;
	limit?: number;
	min_score?: number;
	directory_prefix?: string;
	exclude_patterns?: string[];
	include_patterns?: string[];
	exclude_content_types?: string[];
	include_categories?: string[];
	exclude_categories?: string[];
	call_chain_depth?: number;
	include_call_chain?: boolean;
	enable_rerank?: boolean;
	rerank_max_candidates?: number;
}

// Aggregated Search Types
export interface SubQuery {
	text: string;
	query_type?: QueryType;
	weight?: number;
}

export interface AggregatedSearchRequest {
	project_id?: number;
	project_path?: string;
	sub_queries: SubQuery[];
	limit?: number;
	min_score?: number;
	directory_prefix?: string;
	exclude_content_types?: string[];
	exclude_patterns?: string[];
	include_patterns?: string[];
	include_categories?: string[];
	exclude_categories?: string[];
	enable_rerank?: boolean;
	rerank_max_candidates?: number;
}

export interface CallChainNode {
	function_id: string;
	function_name: string;
	file_path: string;
	depth: number;
	relation_type: string;
	call_line?: number;
}

export interface SearchResultItem {
	entity_ids: number[];
	score: number;
	file_path: string;
	code_chunk: string;
	start_line: number;
	end_line: number;
	entity_type?: string;
	source: string;
	call_chain?: CallChainNode[];
}

export interface SearchResponse {
	success: boolean;
	total: number;
	items: SearchResultItem[];
	elapsed_ms: number;
	sources_used: string[];
}

export interface AggregatedSearchResponse {
	success: boolean;
	total: number;
	items: SearchResultItem[];
	elapsed_ms: number;
	sub_queries_count: number;
	sources_used: string[];
}

export interface EntitySearchResultItem {
	id: number;
	name: string;
	kind: string;
	file_id: number;
	signature?: string;
	span_start_row?: number;
	span_end_row?: number;
	depth?: number;
	parent_id?: number;
	project_id: number;
	rank: number;
}

export const searchApi = {
	search: (request: SearchRequest) =>
		apiClient.post<SearchResponse>('/api/search', request),

	aggregatedSearch: (request: AggregatedSearchRequest) =>
		apiClient.post<AggregatedSearchResponse>('/api/search/aggregated', request),

	entitySearch: (request: { query: string; project_id?: number; project_path?: string; limit?: number; kind_filter?: string }) =>
		apiClient.post<{ success: boolean; total: number; items: EntitySearchResultItem[]; elapsed_ms: number }>('/api/entities/search', request),
};
