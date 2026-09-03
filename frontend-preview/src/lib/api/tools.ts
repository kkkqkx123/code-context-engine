/**
 * Tools API
 * Handles code analysis utilities
 */

import { apiClient, unwrapResponse, type ApiSuccessResponse } from './client';

export interface CompressRequest {
	file_path: string;
	include_entities?: boolean;
	include_groups?: boolean;
}

export interface CompressResponse {
	file_path: string;
	language: string;
	file_hash: string;
	from_cache: boolean;
	semantic_text: string;
}

export interface CompressApiResponse {
	success: boolean;
	error?: string;
	file_path?: string;
	language?: string;
	file_hash?: string;
	from_cache?: boolean;
	semantic_text?: string;
}

export interface BatchCompressRequest {
	file_paths: string[];
	include_entities?: boolean;
	include_groups?: boolean;
	max_concurrency?: number;
}

export interface BatchCompressResponse {
	successes: [string, CompressResponse][];
	failures: [string, string][];
}

export interface DiagnoseRequest {
	code: string;
	language?: string;
	file_name?: string;
	include_ast?: boolean;
}

export interface DiagnoseIssue {
	severity: 'error' | 'warning' | 'info';
	message: string;
	suggestion?: string;
	line?: number;
	column?: number;
}

export interface DiagnoseResponse {
	success: boolean;
	result?: {
		issues: DiagnoseIssue[];
	};
	error?: string;
}

export interface SymbolInfo {
	name: string;
	type: string;
	line_start: number;
	line_end: number;
}

export interface GetSymsRequest {
	project_id: number;
	paths: string[];
}

export interface FileSymbolResult {
	path: string;
	success: boolean;
	symbol_count?: number;
	symbols?: SymbolInfo[];
	error?: string;
}

export interface GetSymbolsResponse {
	results: FileSymbolResult[];
	success_count: number;
	fail_count: number;
}

export interface FindRefsRequest {
	project_id: number;
	path: string;
	line: number;
	column?: number;
	symbol?: string;
	context_lines?: number;
	include_snippet?: boolean;
	include_entity_info?: boolean;
}

export interface GotoDefRequest {
	project_id: number;
	path: string;
	line: number;
	column?: number;
	symbol?: string;
	include_body?: boolean;
}

export interface KeywordSearchRequest {
	query: string;
	project_id?: number;
	top_n?: number;
}

export interface KeywordSearchItem {
	chunk_id: string;
	file_path: string;
	score: number;
	snippet: string;
	highlighted_snippet: string;
}

export interface KeywordSearchData {
	query: string;
	total: number;
	results: KeywordSearchItem[];
}

export interface KeywordSearchResponse {
	success: boolean;
	data?: KeywordSearchData;
	error?: string;
}

export interface ToolApiResponse<T> {
	success: boolean;
	result?: T;
	error?: string;
	relation_info?: Record<string, unknown>;
}

export const toolsApi = {
	// Compress code file
	compress: (data: CompressRequest) =>
		apiClient.post<CompressApiResponse>('/api/tools/compress', data),

	// Batch compress
	batchCompress: (data: BatchCompressRequest) =>
		apiClient.post<BatchCompressResponse>('/api/tools/compress/batch', data),

	// Diagnose code
	diagnose: (data: DiagnoseRequest) =>
		apiClient.post<DiagnoseResponse>('/api/tools/diagnose', data),

	// Extract symbols from files (project-scoped)
	getSymbols: async (data: GetSymsRequest) => {
		const response = await apiClient.post<ToolApiResponse<GetSymbolsResponse>>('/api/tools/symbols', data);
		return unwrapResponse(response);
	},

	// Find symbol references (project-scoped, position-based)
	findReferences: async (data: FindRefsRequest) => {
		const response = await apiClient.post<ToolApiResponse<unknown>>('/api/tools/references', data);
		return unwrapResponse(response);
	},

	// Go to definition (project-scoped, position-based)
	getDefinition: async (data: GotoDefRequest) => {
		const response = await apiClient.post<ToolApiResponse<unknown>>('/api/tools/definition', data);
		return unwrapResponse(response);
	},

	// Keyword search (BM25-based)
	keywordSearch: (data: KeywordSearchRequest) =>
		apiClient.post<KeywordSearchResponse>('/api/tools/keyword-search', data),
};