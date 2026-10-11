/**
 * Dead-letter & retry-queue ops API.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type DeadLetterModuleEntry =
	components['schemas']['DeadLetterModuleEntry'];
export type DeadLetterFileEntry =
	components['schemas']['DeadLetterFileEntry'];
export type DeadLetterListResponse =
	components['schemas']['DeadLetterListResponse'];
export type DeadLetterActionResponse =
	components['schemas']['DeadLetterActionResponse'];
export type RetryQueueStatus =
	components['schemas']['RetryQueueStatusResponse'];
export type RetryQueueDeadResponse =
	components['schemas']['RetryQueueDeadResponse'];
export type RetryQueueDeadEntry =
	components['schemas']['RetryQueueDeadEntry'];
export type RetryQueueDeadClearResponse =
	components['schemas']['RetryQueueDeadClearResponse'];

export const deadLetterApi = {
	// List dead-lettered files of a project
	list: (projectId: number): Promise<DeadLetterListResponse> =>
		call(
			client.GET('/api/project/{id}/dead-letters', {
				params: { path: { id: projectId } },
			}),
		),

	// Retry dead letters (empty files = all candidates)
	retry: (
		projectId: number,
		files: string[],
	): Promise<DeadLetterActionResponse> =>
		call(
			client.POST('/api/project/{id}/dead-letters/retry', {
				params: { path: { id: projectId } },
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
			client.POST('/api/project/{id}/dead-letters/acknowledge', {
				params: { path: { id: projectId } },
				body: {
					file_path: filePath,
					...(module !== undefined ? { module } : {}),
				},
			}),
		),
};

export const retryQueueApi = {
	// Retry queue status (pending + dead counts, global aggregate)
	getStatus: (): Promise<RetryQueueStatus> =>
		call(client.GET('/api/retry-queue')),

	// Dead-lettered queries snapshot
	getDead: (): Promise<RetryQueueDeadResponse> =>
		call(client.GET('/api/retry-queue/dead')),

	// Discard the dead-letter lists of all retry queues
	clearDead: (): Promise<RetryQueueDeadClearResponse> =>
		call(client.DELETE('/api/retry-queue/dead')),
};
