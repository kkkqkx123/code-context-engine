/**
 * Tools API
 * Handles code analysis utilities.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 * Tool endpoints report business errors in-band ({success, result, error}),
 * so each call unwraps the envelope and throws on failure.
 */

import { apiClient, type ApiError } from './client';
import type { components } from './schema';

export type CompressRequest = components['schemas']['CompressRequest'];
export type CompressResult = components['schemas']['CompressResult'];
export type CompressApiResponse = components['schemas']['CompressApiResponse'];
export type BatchCompressRequest = components['schemas']['BatchCompressRequest'];
export type BatchCompressResponse = components['schemas']['BatchCompressResponse'];
export type BatchCompressSuccess = components['schemas']['BatchCompressSuccess'];
export type BatchCompressFailure = components['schemas']['BatchCompressFailure'];
export type DiagnoseRequest = components['schemas']['DiagnoseRequest'];
export type DiagnoseResult = components['schemas']['DiagnoseResult'];
export type DiagnoseApiResponse = components['schemas']['DiagnoseApiResponse'];
export type Diagnostic = components['schemas']['DiagnosticEntry'];
export type AstNodeInfo = components['schemas']['AstNodeInfo'];
export type FoldRequest = components['schemas']['FoldRequest'];
export type FoldResponse = components['schemas']['FoldResponse'];
export type SymbolInfo = components['schemas']['SymbolInfo'];
export type GetSymsRequest = components['schemas']['GetSymbolsRequest'];
export type FileSymbolResult = components['schemas']['FileSymbolResult'];
export type GetSymbolsResponse = components['schemas']['GetSymbolsResponse'];
export type GetSymbolsResult = components['schemas']['GetSymbolsResult'];
export type FindRefsRequest = components['schemas']['FindReferencesRequest'];
export type FindReferencesResult = components['schemas']['FindReferencesResult'];
export type FindReferencesResponse = components['schemas']['FindReferencesResponse'];
export type GotoDefRequest = components['schemas']['GotoDefinitionRequest'];
export type GotoDefinitionResult = components['schemas']['GotoDefinitionResult'];
export type GotoDefinitionResponse = components['schemas']['GotoDefinitionResponse'];
export type KeywordSearchRequest = components['schemas']['KeywordSearchRequest'];
export type KeywordSearchItem = components['schemas']['KeywordSearchItem'];
export type KeywordSearchResult = components['schemas']['KeywordSearchResult'];
export type KeywordSearchApiResponse = components['schemas']['KeywordSearchApiResponse'];

interface InBand<T> {
	success: boolean;
	result?: T | null;
	error?: string | null;
}

function unwrapInBand<T>(response: InBand<T>): T {
	if (!response.success || response.result == null) {
		throw { message: response.error || 'Tool request failed', status: 0 } as ApiError;
	}
	return response.result;
}

export const toolsApi = {
	// Compress code file
	compress: async (data: CompressRequest): Promise<CompressResult> => {
		const response = await apiClient.post<CompressApiResponse>('/api/tools/compress', data);
		return unwrapInBand(response);
	},

	// Batch compress
	batchCompress: (data: BatchCompressRequest) =>
		apiClient.post<BatchCompressResponse>('/api/tools/compress/batch', data),

	// Diagnose code
	diagnose: async (data: DiagnoseRequest): Promise<DiagnoseResult> => {
		const response = await apiClient.post<DiagnoseApiResponse>('/api/tools/diagnose', data);
		return unwrapInBand(response);
	},

	// Fold raw text into a symbol skeleton (stateless)
	fold: (data: FoldRequest) => apiClient.post<FoldResponse>('/api/tools/fold', data),

	// Extract symbols from files (project-scoped)
	getSymbols: async (data: GetSymsRequest): Promise<GetSymbolsResult> => {
		const response = await apiClient.post<GetSymbolsResponse>('/api/tools/symbols', data);
		return unwrapInBand(response);
	},

	// Find symbol references (project-scoped, position-based)
	findReferences: async (data: FindRefsRequest): Promise<FindReferencesResult> => {
		const response = await apiClient.post<FindReferencesResponse>('/api/tools/references', data);
		return unwrapInBand(response);
	},

	// Go to definition (project-scoped, position-based)
	getDefinition: async (data: GotoDefRequest): Promise<GotoDefinitionResult> => {
		const response = await apiClient.post<GotoDefinitionResponse>('/api/tools/definition', data);
		return unwrapInBand(response);
	},

	// Keyword search (BM25-based)
	keywordSearch: async (data: KeywordSearchRequest): Promise<KeywordSearchResult> => {
		const response = await apiClient.post<KeywordSearchApiResponse>('/api/tools/keyword-search', data);
		return unwrapInBand(response);
	},
};
