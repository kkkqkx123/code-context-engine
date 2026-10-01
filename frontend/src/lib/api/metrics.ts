/**
 * Metrics API
 * Handles system metrics export.
 * Wire types come from the generated OpenAPI contract (schema.d.ts).
 */

import { BASE_URL, call, client } from './client';
import type { components } from './schema';

export type MetricsData = Record<string, unknown>;
export type AggregatedMetric = components['schemas']['AggregatedMetric'];
export type CleanupMetricsResponse = components['schemas']['MetricsCleanupResponse'];

export const metricsApi = {
	// Get metrics in JSON format
	getJsonMetrics: (): Promise<MetricsData> => call(client.GET('/api/metrics/json')),

	// Get metrics in Prometheus format.
	// Non-JSON exception: the endpoint answers with Prometheus text exposition,
	// which OpenAPI cannot model as a JSON schema, so it bypasses the typed
	// client and is not covered by codegen.
	getPrometheusMetrics: (): Promise<string> =>
		fetch(`${BASE_URL}/api/metrics`).then((res) => res.text()),

	// Get metrics history
	getHistory: (params: {
		from: string;
		to: string;
		metric?: string;
		project_id?: number;
		operation_type?: string;
	}): Promise<AggregatedMetric[]> =>
		call(
			client.GET('/api/metrics/history', {
				params: {
					query: {
						from: params.from,
						to: params.to,
						metric: params.metric,
						project_id: params.project_id,
						operation_type: params.operation_type
					}
				}
			})
		),

	// Cleanup metrics
	cleanup: (params: { all?: boolean; before?: string }): Promise<CleanupMetricsResponse> => {
		const query: { all?: boolean; before?: string } = {};
		if (params.all !== undefined) query.all = params.all;
		if (params.before !== undefined) query.before = params.before;
		return call(
			client.DELETE('/api/metrics/cleanup', {
				params: { query }
			})
		);
	}
};
