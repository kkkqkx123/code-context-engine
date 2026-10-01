/**
 * Tools API
 * Handles code analysis utilities.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 * Tool endpoints report business errors in-band ({success, result, error}),
 * so each call unwraps the envelope and throws on failure.
 */

import { ApiError, call, client } from './client';
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

/** In-band tool result shape shared by the generated `*ApiResponse` schemas. */
interface InBand<T> {
	success: boolean;
	result?: T | null;
	error?: string | null;
}

function unwrapInBand<T>(response: InBand<T>): T {
	if (!response.success || response.result == null) {
		throw new ApiError(response.error || 'Tool request failed', 0);
	}
	return response.result;
}

export const toolsApi = {
	// Compress code file
	compress: async (data: CompressRequest): Promise<CompressResult> => {
		const response = await call<CompressApiResponse>(
			client.POST('/api/tools/compress', { body: data })
		);
		return unwrapInBand(response);
	},

	// Batch compress
	batchCompress: (data: BatchCompressRequest): Promise<BatchCompressResponse> =>
		call(client.POST('/api/tools/compress/batch', { body: data })),

	// Diagnose code
	diagnose: async (data: DiagnoseRequest): Promise<DiagnoseResult> => {
		const response = await call<DiagnoseApiResponse>(
			client.POST('/api/tools/diagnose', { body: data })
		);
		return unwrapInBand(response);
	},

	// Fold raw text into a symbol skeleton (stateless)
	fold: (data: FoldRequest): Promise<FoldResponse> =>
		call(client.POST('/api/tools/fold', { body: data })),

	// Extract symbols from files (project-scoped)
	getSymbols: async (data: GetSymsRequest): Promise<GetSymbolsResult> => {
		const response = await call<GetSymbolsResponse>(
			client.POST('/api/tools/symbols', { body: data })
		);
		return unwrapInBand(response);
	},

	// Find symbol references (project-scoped, position-based)
	findReferences: async (data: FindRefsRequest): Promise<FindReferencesResult> => {
		const response = await call<FindReferencesResponse>(
			client.POST('/api/tools/references', { body: data })
		);
		return unwrapInBand(response);
	},

	// Go to definition (project-scoped, position-based)
	getDefinition: async (data: GotoDefRequest): Promise<GotoDefinitionResult> => {
		const response = await call<GotoDefinitionResponse>(
			client.POST('/api/tools/definition', { body: data })
		);
		return unwrapInBand(response);
	},

	// Keyword search (BM25-based)
	keywordSearch: async (data: KeywordSearchRequest): Promise<KeywordSearchResult> => {
		const response = await call<KeywordSearchApiResponse>(
			client.POST('/api/tools/keyword-search', { body: data })
		);
		return unwrapInBand(response);
	}
};
