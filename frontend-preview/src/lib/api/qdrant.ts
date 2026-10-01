/**
 * Qdrant Process Management API
 * Handles Qdrant subprocess lifecycle operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type QdrantProcessStatus = components['schemas']['QdrantProcessStatus'];
export type QdrantProcessStatusResponse = components['schemas']['QdrantProcessStatusResponse'];
export type QdrantActionResponse = components['schemas']['QdrantActionResponse'];

export const qdrantApi = {
	/** GET /api/qdrant/process/status */
	getStatus: (): Promise<QdrantProcessStatusResponse> =>
		call(client.GET('/api/qdrant/process/status')),

	/** POST /api/qdrant/process/start */
	start: (): Promise<QdrantActionResponse> => call(client.POST('/api/qdrant/process/start')),

	/** POST /api/qdrant/process/stop */
	stop: (): Promise<QdrantActionResponse> => call(client.POST('/api/qdrant/process/stop')),

	/** POST /api/qdrant/process/restart */
	restart: (): Promise<QdrantActionResponse> =>
		call(client.POST('/api/qdrant/process/restart'))
};
