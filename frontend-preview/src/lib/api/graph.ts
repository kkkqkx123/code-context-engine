/**
 * Graph API
 * Handles relation graph retrieval: ego neighborhoods, explicit subgraphs,
 * two-point paths, connected components and file impact analysis.
 *
 * All endpoints are project-scoped and return node-link structures that map
 * directly onto a graph renderer's element model. Responses carry a
 * `relation_epoch` version marker which callers should use for cache
 * invalidation. Wire types come from the generated OpenAPI contract.
 */

import { apiClient } from './client';
import type { components } from './schema';

export type GraphNode = components['schemas']['GraphNode'];
export type GraphEdge = components['schemas']['GraphEdge'];
export type GraphSubgraphResponse = components['schemas']['GraphSubgraphResponse'];
export type GraphPathResponse = components['schemas']['GraphPathResponse'];
export type GraphComponentsResponse = components['schemas']['GraphComponentsResponse'];
export type GraphImpactResponse = components['schemas']['GraphImpactResponse'];

/** Traversal direction for ego queries. */
export type GraphDirection = 'in' | 'out' | 'both';

/** Ego neighborhood query options. */
export interface EgoOptions {
	entityId: string;
	depth?: number;
	direction?: GraphDirection;
}

/** Two-point path query options. */
export interface GraphPathOptions {
	start: string;
	end: string;
	maxDepth?: number;
}

const DEFAULT_EGO_DEPTH = 2;
const DEFAULT_EGO_DIRECTION: GraphDirection = 'both';
const DEFAULT_PATH_DEPTH = 10;

/** Upper bound accepted by the backend for explicit subgraph queries. */
export const MAX_SUBGRAPH_IDS = 200;

function buildQuery(params: Record<string, string | number | undefined>): string {
	const search = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value === undefined) continue;
		search.set(key, String(value));
	}
	const query = search.toString();
	return query ? `?${query}` : '';
}

export const graphApi = {
	/**
	 * Neighborhood around a single entity. `depth` is clamped by the backend
	 * against the project's configured relation max depth.
	 */
	getEgo: (projectId: number, options: EgoOptions) =>
		apiClient.get<GraphSubgraphResponse>(
			`/api/project/${projectId}/graph/ego${buildQuery({
				entity_id: options.entityId,
				depth: options.depth ?? DEFAULT_EGO_DEPTH,
				direction: options.direction ?? DEFAULT_EGO_DIRECTION,
			})}`
		),

	/** Shortest relation path between two entities. */
	getPath: (projectId: number, options: GraphPathOptions) =>
		apiClient.get<GraphPathResponse>(
			`/api/project/${projectId}/graph/path${buildQuery({
				start: options.start,
				end: options.end,
				max_depth: options.maxDepth ?? DEFAULT_PATH_DEPTH,
			})}`
		),

	/** Subgraph for an explicit set of stable entity ids. */
	getSubgraph: (projectId: number, ids: string[]) => {
		const limited = ids.filter((id) => id.trim().length > 0).slice(0, MAX_SUBGRAPH_IDS);
		return apiClient.get<GraphSubgraphResponse>(
			`/api/project/${projectId}/graph/subgraph${buildQuery({ ids: limited.join(',') })}`
		);
	},

	/** Connected components, used to group entities into communities. */
	getComponents: (projectId: number) =>
		apiClient.get<GraphComponentsResponse>(`/api/project/${projectId}/graph/components`),

	/**
	 * Bulk graph export. The backend defaults to a bounded limit, so callers
	 * should not assume this returns the entire project graph.
	 */
	exportGraph: (projectId: number, limit?: number) =>
		apiClient.get<GraphSubgraphResponse>(
			`/api/project/${projectId}/graph/export${buildQuery({ limit })}`
		),

	/** Dependent entities and impact score for a changed file. */
	getImpact: (projectId: number, file: string) =>
		apiClient.get<GraphImpactResponse>(
			`/api/project/${projectId}/graph/impact${buildQuery({ file })}`
		),
};
