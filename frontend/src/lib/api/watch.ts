/**
 * Watch API
 * Handles file system watching operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type WatchStartRequest = components['schemas']['StartWatchRequest'];
export type StartWatchResponse = components['schemas']['StartWatchResponse'];
export type StopWatchResponse = components['schemas']['StopWatchResponse'];
export type WatchStatus = components['schemas']['WatchStatus'];
export type WatchStatusResponse = components['schemas']['WatchStatusResponse'];

export const watchApi = {
	// Start watching directory
	startWatch: (projectId: number, data: WatchStartRequest) =>
		apiClient.post<StartWatchResponse>(`/api/project/${projectId}/watch/start`, data),

	// Stop watching
	stopWatch: (projectId: number) =>
		apiClient.post<StopWatchResponse>(`/api/project/${projectId}/watch/stop`),

	// Get watch status
	getStatus: (projectId: number) =>
		apiClient.get<WatchStatusResponse>(`/api/project/${projectId}/watch/status`),
};
