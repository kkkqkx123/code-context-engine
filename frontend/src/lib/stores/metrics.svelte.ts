/**
 * Metrics State Store.
 * Rune-based replacement of the previous `writable<MetricsState>` + `metricsActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import { storageApi } from '../api/storage';
import { metricsApi } from '../api/metrics';
import type { StorageStatusDetail } from '../api/storage';

export interface MetricsState {
	storageStatus: StorageStatusDetail | null;
	systemMetrics: Record<string, unknown> | null;
	lastUpdated: Date | null;
	isLoading: boolean;
	error: string | null;
}

let refreshInterval: ReturnType<typeof setInterval> | null = null;

class MetricsStore {
	storageStatus = $state<StorageStatusDetail | null>(null);
	systemMetrics = $state<Record<string, unknown> | null>(null);
	lastUpdated = $state<Date | null>(null);
	isLoading = $state(false);
	error = $state<string | null>(null);

	async loadMetrics() {
		this.isLoading = true;
		this.error = null;

		try {
			const [storageResponse, systemMetrics] = await Promise.all([
				storageApi.getStatus(),
				metricsApi.getJsonMetrics().catch(() => null),
			]);

			this.storageStatus = storageResponse.status;
			this.systemMetrics = systemMetrics;
			this.lastUpdated = new Date();
			this.isLoading = false;
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	startAutoRefresh(intervalMs: number = 30000) {
		if (refreshInterval) {
			clearInterval(refreshInterval);
		}

		this.loadMetrics();

		refreshInterval = setInterval(() => {
			this.loadMetrics();
		}, intervalMs);
	}

	stopAutoRefresh() {
		if (refreshInterval) {
			clearInterval(refreshInterval);
			refreshInterval = null;
		}
	}
}

export const metricsStore = new MetricsStore();
