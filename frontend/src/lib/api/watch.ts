/**
 * Watch API
 * Handles file system watching operations
 */

import { apiClient } from './client';

export interface WatchStartRequest {
	path: string;
	extensions?: string[];
	debounce_ms?: number;
}

export interface WatchStatus {
	active: boolean;
	events_processed: number;
	watched_dirs: string[];
	started_at?: string;
}

export interface WatchStatusResponse {
	success: boolean;
	status: WatchStatus;
}

export const watchApi = {
	// Start watching directory
	startWatch: (projectId: number, data: WatchStartRequest) =>
		apiClient.post(`/api/project/${projectId}/watch/start`, data),

	// Stop watching
	stopWatch: (projectId: number) =>
		apiClient.post(`/api/project/${projectId}/watch/stop`),

	// Get watch status
	getStatus: (projectId: number) =>
		apiClient.get<WatchStatusResponse>(`/api/project/${projectId}/watch/status`),
};