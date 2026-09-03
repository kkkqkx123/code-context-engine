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

export const watchApi = {
	// Start watching directory
	startWatch: (projectId: number, data: WatchStartRequest) =>
		apiClient.post(`/api/project/${projectId}/watch/start`, data),

	// Stop watching
	stopWatch: (projectId: number) =>
		apiClient.post(`/api/project/${projectId}/watch/stop`),

	// Get watch status
	getStatus: (projectId: number) =>
		apiClient.get<WatchStatus>(`/api/project/${projectId}/watch/status`),
};