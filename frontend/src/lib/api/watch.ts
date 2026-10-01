/**
 * Watch API
 * Handles file system watching operations.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type WatchStartRequest = components['schemas']['StartWatchRequest'];
export type StartWatchResponse = components['schemas']['StartWatchResponse'];
export type StopWatchResponse = components['schemas']['StopWatchResponse'];
export type WatchStatus = components['schemas']['WatchStatus'];
export type WatchStatusResponse = components['schemas']['WatchStatusResponse'];

export const watchApi = {
	// Start watching directory
	startWatch: (
		projectId: number,
		data: WatchStartRequest,
	): Promise<StartWatchResponse> =>
		call(
			client.POST('/api/project/{project_id}/watch/start', {
				params: { path: { project_id: projectId } },
				body: data,
			}),
		),

	// Stop watching
	stopWatch: (projectId: number): Promise<StopWatchResponse> =>
		call(
			client.POST('/api/project/{project_id}/watch/stop', {
				params: { path: { project_id: projectId } },
			}),
		),

	// Get watch status
	getStatus: (projectId: number): Promise<WatchStatusResponse> =>
		call(
			client.GET('/api/project/{project_id}/watch/status', {
				params: { path: { project_id: projectId } },
			}),
		),
};
