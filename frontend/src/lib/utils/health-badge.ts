/**
 * Map a boolean-or-unknown health flag to badge label and variant.
 * Shared by the health page cards so unhealthy/unknown rendering stays
 * consistent across the unified status and per-service diagnostics.
 */
import type { BadgeVariant } from '$lib/utils/tone';

export function serviceBadge(healthy: boolean | undefined): {
	label: string;
	variant: Extract<BadgeVariant, 'success' | 'danger' | 'inactive'>;
} {
	if (healthy === undefined) return { label: 'Unknown', variant: 'inactive' };
	return healthy
		? { label: 'Healthy', variant: 'success' }
		: { label: 'Unhealthy', variant: 'danger' };
}
