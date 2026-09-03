/**
 * Health State Store
 * Manages external service health status and retry queue state
 */

import { writable } from 'svelte/store';
import { healthApi } from '../api/health';
import type {
	HealthStatus,
	QdrantHealthStatus,
	EmbeddingHealthStatus,
	Bm25HealthStatus,
	RetryQueueStatus,
} from '../api/health';

// ─── Types ────────────────────────────────────────────────────────

export interface HealthState {
	unified: HealthStatus | null;
	qdrant: QdrantHealthStatus | null;
	embedding: EmbeddingHealthStatus | null;
	bm25: Bm25HealthStatus | null;
	retryQueue: RetryQueueStatus | null;
	lastUpdated: Date | null;
	isLoading: boolean;
	error: string | null;
}

// ─── Store ────────────────────────────────────────────────────────

export const healthState = writable<HealthState>({
	unified: null,
	qdrant: null,
	embedding: null,
	bm25: null,
	retryQueue: null,
	lastUpdated: null,
	isLoading: false,
	error: null,
});

// Auto-refresh interval (15 seconds for health, which should be quicker)
let refreshInterval: ReturnType<typeof setInterval> | null = null;

// ─── Actions ──────────────────────────────────────────────────────

export const healthActions = {
	/** Load unified health status and retry queue status */
	async loadAll() {
		healthState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			const [unified, retryQueue] = await Promise.all([
				healthApi.getHealth(),
				healthApi.getRetryQueueStatus().catch(() => null),
			]);

			healthState.update(state => ({
				...state,
				unified,
				retryQueue,
				lastUpdated: new Date(),
				isLoading: false,
			}));
		} catch (error: any) {
			healthState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	/** Load Qdrant detailed diagnostics */
	async loadQdrantHealth() {
		try {
			const qdrant = await healthApi.getQdrantHealth();
			healthState.update(state => ({ ...state, qdrant }));
		} catch (error: any) {
			healthState.update(state => ({ ...state, error: error.message }));
		}
	},

	/** Load Embedding service health */
	async loadEmbeddingHealth() {
		try {
			const embedding = await healthApi.getEmbeddingHealth();
			healthState.update(state => ({ ...state, embedding }));
		} catch (error: any) {
			healthState.update(state => ({ ...state, error: error.message }));
		}
	},

	/** Load BM25 health */
	async loadBm25Health() {
		try {
			const bm25 = await healthApi.getBm25Health();
			healthState.update(state => ({ ...state, bm25 }));
		} catch (error: any) {
			healthState.update(state => ({ ...state, error: error.message }));
		}
	},

	/** Manually trigger retry queue processing */
	async processRetryQueue() {
		try {
			const result = await healthApi.processRetryQueue();
			// Refresh status after processing
			await healthActions.loadAll();
			return result;
		} catch (error: any) {
			healthState.update(state => ({ ...state, error: error.message }));
			return null;
		}
	},

	/** Clear retry queue */
	async clearRetryQueue() {
		try {
			const result = await healthApi.clearRetryQueue();
			// Refresh status after clearing
			await healthActions.loadAll();
			return result;
		} catch (error: any) {
			healthState.update(state => ({ ...state, error: error.message }));
			return null;
		}
	},

	/** Start auto-refresh (15 second interval) */
	startAutoRefresh(intervalMs: number = 15000) {
		if (refreshInterval) {
			clearInterval(refreshInterval);
		}

		healthActions.loadAll();

		refreshInterval = setInterval(() => {
			healthActions.loadAll();
		}, intervalMs);
	},

	/** Stop auto-refresh */
	stopAutoRefresh() {
		if (refreshInterval) {
			clearInterval(refreshInterval);
			refreshInterval = null;
		}
	},
};