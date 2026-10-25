/**
 * Dead-letter & retry-queue ops API.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from '#api/client';
import type { components } from './schema';

export type DeadLetterModuleEntry =
	components['schemas']['DeadLetterModuleEntry'];
export type DeadLetterFileEntry = components['schemas']['DeadLetterFileEntry'];
export type DeadLetterListResponse =
	components['schemas']['DeadLetterListResponse'];
export type DeadLetterActionResponse =
	components['schemas']['DeadLetterActionResponse'];
export type RetryQueueStatus =
	components['schemas']['RetryQueueStatusResponse'];
export type RetryQueueDeadResponse =
	components['schemas']['RetryQueueDeadResponse'];
export type RetryQueueDeadEntry = components['schemas']['RetryQueueDeadEntry'];
export type RetryQueueDeadClearResponse =
	components['schemas']['RetryQueueDeadClearResponse'];
export type RetryQueueClearResponse =
	components['schemas']['RetryQueueClearResponse'];
export type RetryQueueProcessResponse =
	components['schemas']['RetryQueueProcessResponse'];

export const deadLetterApi = {
	// List dead-lettered files of a project
	list: (projectId: number): Promise<DeadLetterListResponse> =>
		call(
			client.GET('/api/project/{project_id}/dead-letters', {
				params: { path: { project_id: projectId } },
			}),
		),

	// Retry dead letters (empty files = all candidates)
	retry: (
		projectId: number,
		files: string[],
	): Promise<DeadLetterActionResponse> =>
		call(
			client.POST('/api/project/{project_id}/dead-letters/retry', {
				params: { path: { project_id: projectId } },
				body: { files },
			}),
		),

	// Acknowledge dead letters of one file (module omitted = all modules)
	acknowledge: (
		projectId: number,
		filePath: string,
		module?: string,
	): Promise<DeadLetterActionResponse> =>
		call(
			client.POST('/api/project/{project_id}/dead-letters/acknowledge', {
				params: { path: { project_id: projectId } },
				body: {
					file_path: filePath,
					...(module !== undefined ? { module } : {}),
				},
			}),
		),
};

export const retryQueueApi = {
	// Retry queue status (pending + dead counts; scoped when projectId is given)
	getStatus: (projectId?: number | null): Promise<RetryQueueStatus> =>
		call(
			client.GET('/api/retry-queue', {
				params: { query: { project_id: projectId ?? undefined } },
			}),
		),

	// Dead-lettered queries snapshot (scoped when projectId is given)
	getDead: (projectId?: number | null): Promise<RetryQueueDeadResponse> =>
		call(
			client.GET('/api/retry-queue/dead', {
				params: { query: { project_id: projectId ?? undefined } },
			}),
		),

	// Discard the dead-letter lists of the selected retry queues
	clearDead: (projectId?: number | null): Promise<RetryQueueDeadClearResponse> =>
		call(
			client.DELETE('/api/retry-queue/dead', {
				params: { query: { project_id: projectId ?? undefined } },
			}),
		),

	// Clear retry queues (scoped when projectId is given)
	clearQueue: (projectId?: number | null): Promise<RetryQueueClearResponse> =>
		call(
			client.DELETE('/api/retry-queue', {
				params: { query: { project_id: projectId ?? undefined } },
			}),
		),

	// Manually trigger retry queue processing (scoped when projectId is given)
	process: (projectId?: number | null): Promise<RetryQueueProcessResponse> =>
		call(
			client.POST('/api/retry-queue/process', {
				params: { query: { project_id: projectId ?? undefined } },
			}),
		),
};
