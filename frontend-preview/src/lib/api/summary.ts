/**
 * Summary Generation API
 * Generates temporary file summaries without storing them.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type SummaryRequest = components['schemas']['SummaryRequest'];
export type FileSummaryItem = components['schemas']['FileSummaryItem'];
export type SummaryResponse = components['schemas']['SummaryResponse'];

export const summaryApi = {
	/** POST /api/summary — generate file summaries */
	generate: (request: SummaryRequest): Promise<SummaryResponse> =>
		call(client.POST('/api/summary', { body: request }))
};
