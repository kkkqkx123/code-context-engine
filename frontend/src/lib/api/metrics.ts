/**
 * Metrics API
 * Handles system metrics export
 */

import { apiClient } from './client';

export interface MetricsData {
	[key: string]: any;
}

export interface AggregatedMetric {
	timestamp: string;
	metric_name: string;
	labels_json?: string | null;
	count: number;
	avg?: number | null;
	median?: number | null;
	max?: number | null;
	p90?: number | null;
	p99?: number | null;
	project_id?: number;
	operation_type?: string;
}

export interface CleanupMetricsResponse {
	success: boolean;
	deleted_count: number;
}

export const metricsApi = {
	// Get metrics in JSON format
	getJsonMetrics: () =>
		apiClient.get<MetricsData>('/api/metrics/json'),

	// Get metrics in Prometheus format
	getPrometheusMetrics: () =>
		fetch(`${apiClient['baseUrl']}/api/metrics`).then(res => res.text()),

	// Get metrics history
	getHistory: (params: { from: string; to: string; metric?: string; project_id?: number; operation_type?: string }) =>
		apiClient.get<AggregatedMetric[]>(`/api/metrics/history?from=${encodeURIComponent(params.from)}&to=${encodeURIComponent(params.to)}${params.metric ? `&metric=${encodeURIComponent(params.metric)}` : ''}${params.project_id !== undefined ? `&project_id=${params.project_id}` : ''}${params.operation_type ? `&operation_type=${encodeURIComponent(params.operation_type)}` : ''}`),

	// Cleanup metrics
	cleanup: (params: { all?: boolean; before?: string }) => {
		const query = params.all
			? '?all=true'
			: params.before
				? `?before=${encodeURIComponent(params.before)}`
				: '';
		return apiClient.delete<CleanupMetricsResponse>(`/api/metrics/cleanup${query}`);
	},
};
