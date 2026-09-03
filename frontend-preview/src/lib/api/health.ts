/**
 * Health & Retry Queue API
 * Provides health monitoring for external services and retry queue management
 */

import { apiClient } from './client';

// ─── Health Check Types ───────────────────────────────────────────

export interface ServiceStatus {
	reachable: boolean;
	message: string;
}

export interface HealthStatus {
	healthy: boolean;
	qdrant: ServiceStatus;
	bm25: ServiceStatus;
	embedding: ServiceStatus;
}

export interface QdrantDiagnostic {
	reachable: boolean;
	version: string | null;
	collection_exists: boolean;
	points_count: number;
	error: string | null;
}

export interface QdrantHealthStatus {
	healthy: boolean;
	circuit_breaker: string;
	diagnostic: QdrantDiagnostic;
}

export interface EmbeddingHealthStatus {
	healthy: boolean;
	model_name: string | null;
	message: string;
}

export interface Bm25HealthStatus {
	enabled: boolean;
	connected: boolean;
	index_path: string | null;
}

// ─── Retry Queue Types ────────────────────────────────────────────

export interface RetryQueueStatus {
	pending_count: number;
	is_empty: boolean;
}

export interface RetryQueueProcessResponse {
	processed: number;
	message: string;
}

export interface RetryQueueClearResponse {
	cleared: number;
	message: string;
}

// ─── API ──────────────────────────────────────────────────────────

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
