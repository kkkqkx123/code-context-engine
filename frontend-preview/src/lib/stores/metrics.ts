/**
 * Metrics State Store
 * Manages system metrics and health status
 */

import { writable } from 'svelte/store';
import { storageApi } from '../api/storage';
import { metricsApi } from '../api/metrics';
import type { StorageStatusDetail } from '../api/storage';

export interface MetricsState {
	storageStatus: StorageStatusDetail | null;
	systemMetrics: any | null;
	lastUpdated: Date | null;
	isLoading: boolean;
	error: string | null;
}

export const metricsState = writable<MetricsState>({
	storageStatus: null,
	systemMetrics: null,
	lastUpdated: null,
	isLoading: false,
	error: null,
});

// Auto-refresh interval (30 seconds)
let refreshInterval: ReturnType<typeof setInterval> | null = null;

// Actions
export const metricsActions = {
	async loadMetrics() {
		metricsState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			const [storageResponse, systemMetrics] = await Promise.all([
				storageApi.getStatus(),
				metricsApi.getJsonMetrics().catch(() => null),
			]);

			metricsState.update(state => ({
				...state,
				storageStatus: storageResponse.status,
				systemMetrics,
				lastUpdated: new Date(),
				isLoading: false,
			}));
		} catch (error: any) {
			metricsState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	startAutoRefresh(intervalMs: number = 30000) {
		if (refreshInterval) {
			clearInterval(refreshInterval);
		}

		metricsActions.loadMetrics();

		refreshInterval = setInterval(() => {
			metricsActions.loadMetrics();
		}, intervalMs);
	},

	stopAutoRefresh() {
		if (refreshInterval) {
			clearInterval(refreshInterval);
			refreshInterval = null;
		}
	},
};