/**
 * Graph State Store
 *
 * Owns the working set of graph elements shown by the graph explorer, plus the
 * filters applied on top of them. Elements are accumulated incrementally as the
 * user expands neighborhoods, and the server-side relation epoch is tracked so
 * a rebuilt index invalidates the accumulated graph automatically.
 */

import { writable, get } from 'svelte/store';
import {
	graphApi,
	type GraphComponentsResponse,
	type GraphDirection,
	type GraphEdge,
	type GraphImpactResponse,
	type GraphNode,
	type GraphPathResponse,
} from '../api/graph';
import { ApiError, type SymbolCandidate } from '../api/client';
import {
	edgeElementId,
	relationDomain,
	toElementEdge,
	toElementNode,
	type GraphElement,
	type RelationDomain,
} from '../utils/graph-style';
import { currentProjectId } from './project';

/** Maximum number of nodes held in memory before expansion is refused. */
export const MAX_RENDERED_NODES = 800;

/** Structured error attached to GraphActionResult when a graph action fails. */
export interface GraphError {
	/** Stable machine-readable code for UI branching and metrics. */
	code: string;
	/** Human-readable message suitable for end-user display. Preserves the
	 *  original backend message instead of collapsing everything to a generic
	 *  string. */
	message: string;
	/** Optional structured detail such as HTTP status or the underlying thrown
	 *  value, for debugging and for callers that need to react precisely. */
	details?: unknown;
	/** Disambiguation candidates for an AMBIGUOUS_SYMBOL seed error. */
	candidates?: SymbolCandidate[];
}

/**
 * Uniform result type returned by every graph action. Callers can branch on
 * `ok` without guesswork about whether null, 0 or an empty array means
 * "not found" vs "request failed".
 */
export type GraphActionResult<T> =
	{ ok: true; value: T } | { ok: false; error: GraphError };

export interface GraphMeta {
	/** Relation epoch reported by the most recent successful response. */
	epoch: number;
	/** Entity the graph was last centered on. */
	focusId: string | null;
	/** Component id (index into `components`) per node id. */
	communities: Record<string, number>;
	/** Relation epoch at which `communities` was last fetched. */
	communitiesEpoch: number;
	/** Direct dependents of the analyzed file. */
	impactDirect: string[];
	/** Transitive dependents of the analyzed file. */
	impactTransitive: string[];
	/** File the impact analysis was run for. */
	impactFile: string | null;
}

export interface GraphFilters {
	domains: RelationDomain[];
	kinds: string[];
	search: string;
	hideAmbiguous: boolean;
}

export interface GraphState {
	projectId: number;
	nodes: GraphNode[];
	edges: GraphEdge[];
	elements: GraphElement[];
	loading: boolean;
	error: GraphError | null;
	truncated: boolean;
	filters: GraphFilters;
	meta: GraphMeta;
}

const ALL_DOMAINS: RelationDomain[] = [
	'call',
	'dependency',
	'structural',
	'reference',
	'template',
	'other',
];

const initialState: GraphState = {
	projectId: get(currentProjectId),
	nodes: [],
	edges: [],
	elements: [],
	loading: false,
	error: null,
	truncated: false,
	filters: {
		domains: [...ALL_DOMAINS],
		kinds: [],
		search: '',
		hideAmbiguous: false,
	},
	meta: {
		epoch: 0,
		focusId: null,
		communities: {},
		communitiesEpoch: 0,
		impactDirect: [],
		impactTransitive: [],
		impactFile: null,
	},
};

export const graphState = writable<GraphState>(initialState);

function toGraphError(
	error: unknown,
	fallback = 'Graph request failed',
): GraphError {
	// Preserve structured shape from fetch helpers.
	if (error instanceof ApiError) {
		const base: GraphError = {
			code: error.code ?? statusToCode(error.status),
			message: error.message,
			details: { status: error.status },
		};
		if (error.code === 'AMBIGUOUS_SYMBOL') {
			base.candidates = error.symbolCandidates();
		}
		return base;
	}
	if (error && typeof error === 'object') {
		const obj = error as Record<string, unknown>;
		if ('message' in obj && typeof obj.message === 'string') {
			if ('status' in obj && typeof obj.status === 'number') {
				return {
					code: statusToCode(obj.status as number),
					message: obj.message,
					details: { status: obj.status, url: obj.url },
				};
			}
			return { code: 'UNKNOWN', message: obj.message, details: error };
		}
		if ('code' in obj && typeof obj.code === 'string') {
			return { code: obj.code, message: fallback, details: error };
		}
	}
	return { code: 'UNKNOWN', message: fallback, details: error };
}

function statusToCode(status: number): string {
	if (status === 404) return 'NOT_FOUND';
	if (status >= 400 && status < 500) return 'INVALID_ARGUMENT';
	if (status >= 500) return 'SERVER_ERROR';
	return 'NETWORK';
}

/**
 * Replace the working set. Used when the focus entity changes, or when the
 * server reports a newer relation epoch than the accumulated data.
 */
function replaceGraph(
	nodes: GraphNode[],
	edges: GraphEdge[],
	epoch: number,
	focusId: string | null,
): void {
	graphState.update((state) => ({
		...state,
		nodes,
		edges,
		elements: [...nodes.map(toElementNode), ...edges.map(toElementEdge)],
		meta: { ...state.meta, epoch, focusId },
		error: null,
	}));
}

/**
 * Merge a newly fetched neighborhood into the working set.
 *
 * Nodes are keyed by id and edges by their derived element id, so expansion is
 * idempotent: re-fetching a neighborhood the user already expanded adds
 * nothing. When the response carries a newer epoch the previous data belongs to
 * a stale index and is discarded instead of merged.
 */
function mergeGraph(
	nodes: GraphNode[],
	edges: GraphEdge[],
	epoch: number,
	focusId: string | null,
): number {
	const state = get(graphState);
	if (epoch > state.meta.epoch && state.meta.epoch !== 0) {
		replaceGraph(nodes, edges, epoch, focusId);
		return nodes.length;
	}

	const knownNodes = new Set(state.nodes.map((node) => node.id));
	const knownEdges = new Set(state.edges.map(edgeElementId));

	const freshNodes = nodes.filter((node) => !knownNodes.has(node.id));
	const freshEdges = edges.filter(
		(edge) => !knownEdges.has(edgeElementId(edge)),
	);

	if (freshNodes.length === 0 && freshEdges.length === 0) {
		return 0;
	}

	graphState.update((current) => ({
		...current,
		nodes: [...current.nodes, ...freshNodes],
		edges: [...current.edges, ...freshEdges],
		elements: [
			...current.elements,
			...freshNodes.map(toElementNode),
			...freshEdges.map(toElementEdge),
		],
		meta: { ...current.meta, epoch, focusId: focusId ?? current.meta.focusId },
	}));

	return freshNodes.length;
}

export const graphActions = {
	/** Load the neighborhood of an entity and replace the working set. */
	async loadEgo(
		entityId: string,
		depth = 2,
		direction: GraphDirection = 'both',
		projectId?: number,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({
			...state,
			projectId: pid,
			loading: true,
			error: null,
		}));
		try {
			const response = await graphApi.getEgo(pid, {
				entityId,
				depth,
				direction,
			});
			replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				entityId,
			);
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, loading: false, error: ge }));
			return { ok: false, error: ge };
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/**
	 * Expand the graph by one hop around an entity, merging the result into the
	 * existing working set. Returns the number of newly added nodes.
	 */
	async expand(
		entityId: string,
		depth = 1,
		direction: GraphDirection = 'both',
	): Promise<GraphActionResult<number>> {
		const state = get(graphState);
		if (state.nodes.length >= MAX_RENDERED_NODES) {
			graphState.update((current) => ({ ...current, truncated: true }));
			const ge: GraphError = {
				code: 'LIMIT_EXCEEDED',
				message: `Render limit of ${MAX_RENDERED_NODES} nodes reached. Reload a smaller seed to expand further.`,
			};
			return { ok: false, error: ge };
		}
		graphState.update((current) => ({ ...current, loading: true }));
		try {
			const response = await graphApi.getEgo(state.projectId, {
				entityId,
				depth,
				direction,
			});
			return {
				ok: true,
				value: mergeGraph(
					response.nodes,
					response.edges,
					response.relation_epoch,
					entityId,
				),
			};
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((current) => ({ ...current, error: ge }));
			return { ok: false, error: ge };
		} finally {
			graphState.update((current) => ({ ...current, loading: false }));
		}
	},

	/** Replace the working set with an explicit subgraph. */
	async loadSubgraph(
		ids: string[],
		projectId?: number,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({
			...state,
			projectId: pid,
			loading: true,
			error: null,
		}));
		try {
			const response = await graphApi.getSubgraph(pid, ids);
			replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				null,
			);
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, loading: false, error: ge }));
			return { ok: false, error: ge };
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/**
	 * Replace the working set with the shortest relation path between two
	 * entities. When no path exists the working set is left untouched and the
	 * error field carries a NO_PATH GraphError, distinguishable from an
	 * actual transport failure.
	 */
	async loadPath(
		start: string,
		end: string,
		maxDepth = 10,
		projectId?: number,
	): Promise<GraphActionResult<GraphPathResponse>> {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({
			...state,
			projectId: pid,
			loading: true,
			error: null,
		}));
		try {
			const response = await graphApi.getPath(pid, { start, end, maxDepth });
			if (response.path_found) {
				replaceGraph(
					response.nodes ?? [],
					response.edges ?? [],
					response.relation_epoch,
					start,
				);
			} else {
				const ge: GraphError = {
					code: 'NO_PATH',
					message: `No relation path found between '${start}' and '${end}' within depth ${maxDepth}.`,
				};
				graphState.update((state) => ({ ...state, error: ge }));
				return { ok: false, error: ge };
			}
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, loading: false, error: ge }));
			return { ok: false, error: ge };
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/** Load a bounded slice of the project graph. */
	async loadOverview(
		limit?: number,
		projectId?: number,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({
			...state,
			projectId: pid,
			loading: true,
			error: null,
		}));
		try {
			const response = await graphApi.exportGraph(pid, limit);
			replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				null,
			);
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, loading: false, error: ge }));
			return { ok: false, error: ge };
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/**
	 * Fetch connected components and map every node id to its component index.
	 * Components describe communities, which the canvas uses for grouping and
	 * the details panel uses for context.
	 */
	async loadComponents(
		projectId?: number,
	): Promise<GraphActionResult<GraphComponentsResponse>> {
		const pid = projectId ?? get(currentProjectId);
		try {
			const response = await graphApi.getComponents(pid);
			const communities: Record<string, number> = {};
			response.components.forEach((component, index) => {
				for (const nodeId of component) {
					communities[nodeId] = index;
				}
			});
			graphState.update((state) => ({
				...state,
				meta: {
					...state.meta,
					communities,
					communitiesEpoch: response.relation_epoch,
				},
			}));
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, error: ge }));
			return { ok: false, error: ge };
		}
	},

	/**
	 * Fetch connected components only when the relation index has moved past
	 * the epoch the current community data was built from. A no-op when the
	 * graph data is unchanged, so repeated seed loads do not re-fetch the
	 * full component list.
	 */
	async loadComponentsIfStale(
		projectId?: number,
	): Promise<GraphActionResult<GraphComponentsResponse>> {
		const state = get(graphState);
		if (state.meta.communitiesEpoch >= state.meta.epoch) {
			return {
				ok: true,
				value: {
					components: [],
					relation_epoch: state.meta.epoch,
					success: true,
				},
			};
		}
		return graphActions.loadComponents(projectId);
	},

	/** Run impact analysis for a changed file and record the result. */
	async loadImpact(
		file: string,
		projectId?: number,
	): Promise<GraphActionResult<GraphImpactResponse>> {
		const pid = projectId ?? get(currentProjectId);
		try {
			const response = await graphApi.getImpact(pid, file);
			graphState.update((state) => ({
				...state,
				meta: {
					...state.meta,
					impactFile: response.changed_file,
					impactDirect: response.direct_dependents,
					impactTransitive: response.indirect_dependents,
				},
			}));
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			graphState.update((state) => ({ ...state, error: ge }));
			return { ok: false, error: ge };
		}
	},

	/** Clear the recorded impact analysis highlight. */
	clearImpact() {
		graphState.update((state) => ({
			...state,
			meta: {
				...state.meta,
				impactFile: null,
				impactDirect: [],
				impactTransitive: [],
			},
		}));
	},

	setFilters(patch: Partial<GraphFilters>) {
		graphState.update((state) => ({
			...state,
			filters: { ...state.filters, ...patch },
		}));
	},

	toggleDomain(domain: RelationDomain) {
		graphState.update((state) => {
			const active = state.filters.domains.includes(domain)
				? state.filters.domains.filter((item) => item !== domain)
				: [...state.filters.domains, domain];
			return { ...state, filters: { ...state.filters, domains: active } };
		});
	},

	setSearch(search: string) {
		graphState.update((state) => ({
			...state,
			filters: { ...state.filters, search },
		}));
	},

	/** Drop the working set, for example after switching projects. */
	reset(projectId?: number) {
		graphState.set({
			...initialState,
			projectId: projectId ?? get(currentProjectId),
			filters: { ...initialState.filters, domains: [...ALL_DOMAINS] },
		});
	},
};

/** Distinct relation domains present in the current edge set. */
export function activeDomains(edges: GraphEdge[]): Set<RelationDomain> {
	const domains = new Set<RelationDomain>();
	for (const edge of edges) {
		domains.add(relationDomain(edge.relation));
	}
	return domains;
}
