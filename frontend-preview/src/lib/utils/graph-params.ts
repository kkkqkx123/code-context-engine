/**
 * Pure helpers shared by the graph explorer page and its panels:
 * seed-input parsing, numeric clamps and the server-side filter shape.
 */
import type { GraphFilter } from '$lib/api/graph';

export type ExternalEdgesMode = 'default' | 'on' | 'off';

/** Clamp the overview node limit into [1, 10000] with a safe fallback. */
export function clampOverviewLimit(value: number): number {
	if (!Number.isFinite(value)) return 400;
	return Math.min(10000, Math.max(1, Math.floor(value)));
}

/**
 * Clamp a user-entered positive limit, falling back when non-finite.
 * Used by cycles / structural / impact depth inputs.
 */
export function clampPositive(value: number, fallback: number, max: number): number {
	if (!Number.isFinite(value)) return fallback;
	return Math.min(max, Math.max(1, Math.floor(value)));
}

/**
 * Build the server-side traversal filter from the advanced filter controls.
 * Returns undefined when every control is at its default so the request
 * layer omits the parameters entirely. Domains are never included here —
 * domain visibility stays purely client-side.
 */
export function buildServerFilter(
	relationTypes: string,
	includeExternal: ExternalEdgesMode,
): GraphFilter | undefined {
	const filter: GraphFilter = {};
	const trimmed = relationTypes.trim();
	if (trimmed) {
		filter.relationTypes = trimmed;
	}
	if (includeExternal !== 'default') {
		filter.includeExternal = includeExternal === 'on';
	}
	return Object.keys(filter).length > 0 ? filter : undefined;
}

/** Split a comma-separated entity id list into trimmed, non-empty ids. */
export function parseEntityIds(raw: string): string[] {
	return raw
		.split(',')
		.map((s) => s.trim())
		.filter(Boolean);
}
