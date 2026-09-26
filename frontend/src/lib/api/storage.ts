/**
 * Storage API
 * Handles storage management and status queries.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type StorageComponentStatus = components['schemas']['StorageComponentStatus'];
export type QdrantProcessInfo = components['schemas']['QdrantProcessInfo'];
export type StorageStatusDetail = components['schemas']['StorageStatus'];
export type StorageStatusResponse = components['schemas']['StorageStatusResponse'];

export const storageApi = {
	// Get storage health status
	getStatus: () =>
		apiClient.get<StorageStatusResponse>('/api/storage/status'),
};
