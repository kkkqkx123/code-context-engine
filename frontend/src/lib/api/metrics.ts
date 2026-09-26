/**
 * Metrics API
 * Handles system metrics export.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { apiClient } from './client';
import type { components } from './schema';

export type MetricsData = Record<string, unknown>;
export type AggregatedMetric = components['schemas']['AggregatedMetric'];
export type CleanupMetricsResponse = components['schemas']['MetricsCleanupResponse'];

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
