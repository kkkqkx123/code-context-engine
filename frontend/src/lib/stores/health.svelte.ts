/**
 * Health State Store.
 * Rune-based replacement of the previous `writable<HealthState>` + `healthActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import { healthApi } from '../api/health';
import type {
	HealthStatus,
	QdrantHealthStatus,
	EmbeddingHealthStatus,
	Bm25HealthStatus,
	RetryQueueStatus,
} from '../api/health';

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

let refreshInterval: ReturnType<typeof setInterval> | null = null;

class HealthStore {
	unified = $state<HealthStatus | null>(null);
	qdrant = $state<QdrantHealthStatus | null>(null);
	embedding = $state<EmbeddingHealthStatus | null>(null);
	bm25 = $state<Bm25HealthStatus | null>(null);
	retryQueue = $state<RetryQueueStatus | null>(null);
	lastUpdated = $state<Date | null>(null);
	isLoading = $state(false);
	error = $state<string | null>(null);

	/** Load unified health status and retry queue status */
	async loadAll() {
		this.isLoading = true;
		this.error = null;

		try {
			const [unified, retryQueue] = await Promise.all([
				healthApi.getHealth(),
				healthApi.getRetryQueueStatus().catch(() => null),
			]);

			this.unified = unified;
			this.retryQueue = retryQueue;
			this.lastUpdated = new Date();
			this.isLoading = false;
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	/** Load Qdrant detailed diagnostics */
	async loadQdrantHealth() {
		try {
			const qdrant = await healthApi.getQdrantHealth();
			this.qdrant = qdrant;
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	/** Load Embedding service health */
	async loadEmbeddingHealth() {
		try {
			const embedding = await healthApi.getEmbeddingHealth();
			this.embedding = embedding;
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	/** Load BM25 health */
	async loadBm25Health() {
		try {
			const bm25 = await healthApi.getBm25Health();
			this.bm25 = bm25;
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	/** Manually trigger retry queue processing */
	async processRetryQueue() {
		try {
			const result = await healthApi.processRetryQueue();
			// Refresh status after processing
			await this.loadAll();
			return result;
		} catch (error) {
			this.error = errorMessage(error);
			return null;
		}
	}

	/** Clear retry queue */
	async clearRetryQueue() {
		try {
			const result = await healthApi.clearRetryQueue();
			// Refresh status after clearing
			await this.loadAll();
			return result;
		} catch (error) {
			this.error = errorMessage(error);
			return null;
		}
	}

	/** Start auto-refresh (15 second interval) */
	startAutoRefresh(intervalMs: number = 15000) {
		if (refreshInterval) {
			clearInterval(refreshInterval);
		}

		this.loadAll();

		refreshInterval = setInterval(() => {
			this.loadAll();
		}, intervalMs);
	}

	/** Stop auto-refresh */
	stopAutoRefresh() {
		if (refreshInterval) {
			clearInterval(refreshInterval);
			refreshInterval = null;
		}
	}
}

export const healthStore = new HealthStore();
