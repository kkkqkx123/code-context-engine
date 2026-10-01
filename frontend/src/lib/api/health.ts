/**
 * Health & Retry Queue API
 * Provides health monitoring for external services and retry queue management.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { call, client } from './client';
import type { components } from './schema';

export type ServiceStatus = components['schemas']['ServiceStatus'];
export type HealthStatus = components['schemas']['HealthStatus'];
export type QdrantDiagnostic = components['schemas']['QdrantDiagnostic'];
export type QdrantHealthStatus = components['schemas']['QdrantHealthResponse'];
export type EmbeddingHealthStatus =
	components['schemas']['EmbeddingHealthResponse'];
export type Bm25HealthStatus = components['schemas']['Bm25HealthResponse'];
export type RetryQueueStatus =
	components['schemas']['RetryQueueStatusResponse'];
export type RetryQueueProcessResponse =
	components['schemas']['RetryQueueProcessResponse'];
export type RetryQueueClearResponse =
	components['schemas']['RetryQueueClearResponse'];

export const healthApi = {
	// Unified health check
	getHealth: (): Promise<HealthStatus> => call(client.GET('/api/health')),

	// Qdrant detailed diagnostics
	getQdrantHealth: (): Promise<QdrantHealthStatus> =>
		call(client.GET('/api/health/qdrant')),

	// Embedding service health
	getEmbeddingHealth: (): Promise<EmbeddingHealthStatus> =>
		call(client.GET('/api/health/embedding')),

	// BM25 index health
	getBm25Health: (): Promise<Bm25HealthStatus> =>
		call(client.GET('/api/health/bm25')),

	// Retry queue status
	getRetryQueueStatus: (): Promise<RetryQueueStatus> =>
		call(client.GET('/api/retry-queue')),

	// Manually trigger retry queue processing
	processRetryQueue: (): Promise<RetryQueueProcessResponse> =>
		call(client.POST('/api/retry-queue/process')),

	// Clear retry queue
	clearRetryQueue: (): Promise<RetryQueueClearResponse> =>
		call(client.DELETE('/api/retry-queue')),
};
