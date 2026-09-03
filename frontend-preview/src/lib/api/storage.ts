/**
 * Storage API
 * Handles storage management and status queries
 */

import { apiClient } from './client';

export interface StorageComponentStatus {
	connected: boolean;
	item_count: number;
	disk_usage_mb: number;
	version?: string;
	last_error?: string;
}

export interface QdrantProcessInfo {
	managed: boolean;
	status: string;
	running: boolean;
}

export interface StorageStatusDetail {
	vector_storage: StorageComponentStatus;
	bm25_storage: StorageComponentStatus;
	relation_storage: StorageComponentStatus;
	cache_storage: StorageComponentStatus;
	total_disk_usage_mb: number;
	process_status?: QdrantProcessInfo;
}

export interface StorageStatusResponse {
	success: boolean;
	status: StorageStatusDetail;
}

export const storageApi = {
	// Get storage health status
	getStatus: () =>
		apiClient.get<StorageStatusResponse>('/api/storage/status'),
};
