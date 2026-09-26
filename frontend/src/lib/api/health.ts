/**
 * Health & Retry Queue API
 * Provides health monitoring for external services and retry queue management.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type ServiceStatus = components['schemas']['ServiceStatus'];
export type HealthStatus = components['schemas']['HealthStatus'];
export type QdrantDiagnostic = components['schemas']['QdrantDiagnostic'];
export type QdrantHealthStatus = components['schemas']['QdrantHealthResponse'];
export type EmbeddingHealthStatus = components['schemas']['EmbeddingHealthResponse'];
export type Bm25HealthStatus = components['schemas']['Bm25HealthResponse'];
export type RetryQueueStatus = components['schemas']['RetryQueueStatusResponse'];
export type RetryQueueProcessResponse = components['schemas']['RetryQueueProcessResponse'];
export type RetryQueueClearResponse = components['schemas']['RetryQueueClearResponse'];

export const healthApi = {
	// Unified health check
	getHealth: () => apiClient.get<HealthStatus>('/api/health'),

	// Qdrant detailed diagnostics
	getQdrantHealth: () => apiClient.get<QdrantHealthStatus>('/api/health/qdrant'),

	// Embedding service health
	getEmbeddingHealth: () => apiClient.get<EmbeddingHealthStatus>('/api/health/embedding'),

	// BM25 index health
	getBm25Health: () => apiClient.get<Bm25HealthStatus>('/api/health/bm25'),

	// Retry queue status
	getRetryQueueStatus: () => apiClient.get<RetryQueueStatus>('/api/retry-queue'),

	// Manually trigger retry queue processing
	processRetryQueue: () =>
		apiClient.post<RetryQueueProcessResponse>('/api/retry-queue/process', {}),

	// Clear retry queue
	clearRetryQueue: () =>
		apiClient.delete<RetryQueueClearResponse>('/api/retry-queue'),
};
