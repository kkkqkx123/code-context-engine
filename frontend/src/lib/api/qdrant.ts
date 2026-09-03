/**
 * Qdrant Process Management API
 * Handles Qdrant subprocess lifecycle operations
 */

import { apiClient } from './client';

/** Mirrors cce_infrastructure::storage::qdrant::QdrantProcessStatus */
export type QdrantProcessStatus =
	| 'Idle'
	| 'Starting'
	| 'Running'
	| 'Stopping'
	| 'Crashed'
	| 'Stopped'
	| { Failed: string };

export interface QdrantProcessStatusResponse {
	managed: boolean;
	status: QdrantProcessStatus;
}

export interface QdrantActionResponse {
	success: boolean;
	message: string;
	status: QdrantProcessStatus;
}

export const qdrantApi = {
	/** GET /api/qdrant/process/status */
	getStatus: () =>
		apiClient.get<QdrantProcessStatusResponse>('/api/qdrant/process/status'),

	/** POST /api/qdrant/process/start */
	start: () =>
		apiClient.post<QdrantActionResponse>('/api/qdrant/process/start'),

	/** POST /api/qdrant/process/stop */
	stop: () =>
		apiClient.post<QdrantActionResponse>('/api/qdrant/process/stop'),

	/** POST /api/qdrant/process/restart */
	restart: () =>
		apiClient.post<QdrantActionResponse>('/api/qdrant/process/restart'),
};