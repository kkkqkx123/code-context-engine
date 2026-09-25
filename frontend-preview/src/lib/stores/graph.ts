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
	type GraphNode
} from '../api/graph';
import {
	edgeElementId,
	relationDomain,
	toElementEdge,
	toElementNode,
	type GraphElement,
	type RelationDomain
} from '../utils/graph-style';
import { currentProjectId } from './project';

/** Maximum number of nodes held in memory before expansion is refused. */
export const MAX_RENDERED_NODES = 800;

export interface GraphMeta {
	/** Relation epoch reported by the most recent successful response. */
	epoch: number;
	/** Entity the graph was last centered on. */
	focusId: string | null;
	/** Component id (index into `components`) per node id. */
	communities: Record<string, number>;
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
	error: string | null;
	truncated: boolean;
	filters: GraphFilters;
	meta: GraphMeta;
}

const ALL_DOMAINS: RelationDomain[] = ['call', 'dependency', 'structural', 'reference', 'other'];

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
		hideAmbiguous: false
	},
	meta: {
		epoch: 0,
		focusId: null,
		communities: {},
		impactDirect: [],
		impactTransitive: [],
		impactFile: null
	}
};

export const graphState = writable<GraphState>(initialState);

function errorMessage(error: unknown): string {
	if (error && typeof error === 'object' && 'message' in error) {
		return String((error as { message: unknown }).message);
	}
	return 'Graph request failed';
}

/**
 * Replace the working set. Used when the focus entity changes, or when the
 * server reports a newer relation epoch than the accumulated data.
 */
function replaceGraph(
	nodes: GraphNode[],
	edges: GraphEdge[],
	epoch: number,
	focusId: string | null
): void {
	graphState.update((state) => ({
		...state,
		nodes,
		edges,
		elements: [...nodes.map(toElementNode), ...edges.map(toElementEdge)],
		meta: { ...state.meta, epoch, focusId },
		error: null
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
	focusId: string | null
): number {
	const state = get(graphState);
	if (epoch > state.meta.epoch && state.meta.epoch !== 0) {
		replaceGraph(nodes, edges, epoch, focusId);
		return nodes.length;
	}

	const knownNodes = new Set(state.nodes.map((node) => node.id));
	const knownEdges = new Set(state.edges.map(edgeElementId));

	const freshNodes = nodes.filter((node) => !knownNodes.has(node.id));
	const freshEdges = edges.filter((edge) => !knownEdges.has(edgeElementId(edge)));

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
			...freshEdges.map(toElementEdge)
		],
		meta: { ...current.meta, epoch, focusId: focusId ?? current.meta.focusId }
	}));

	return freshNodes.length;
}

export const graphActions = {
	/** Load the neighborhood of an entity and replace the working set. */
	async loadEgo(entityId: string, depth = 2, direction: GraphDirection = 'both', projectId?: number) {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({ ...state, projectId: pid, loading: true, error: null }));
		try {
			const response = await graphApi.getEgo(pid, { entityId, depth, direction });
			replaceGraph(response.nodes, response.edges, response.relation_epoch, entityId);
			return response.nodes.length;
		} catch (error) {
			graphState.update((state) => ({ ...state, loading: false, error: errorMessage(error) }));
			return 0;
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/**
	 * Expand the graph by one hop around an entity, merging the result into the
	 * existing working set. Returns the number of newly added nodes.
	 */
	async expand(entityId: string, depth = 1, direction: GraphDirection = 'both') {
		const state = get(graphState);
		if (state.nodes.length >= MAX_RENDERED_NODES) {
			graphState.update((current) => ({ ...current, truncated: true }));
			return 0;
		}
		graphState.update((current) => ({ ...current, loading: true }));
		try {
			const response = await graphApi.getEgo(state.projectId, { entityId, depth, direction });
			return mergeGraph(response.nodes, response.edges, response.relation_epoch, entityId);
		} catch (error) {
			graphState.update((current) => ({ ...current, error: errorMessage(error) }));
			return 0;
		} finally {
			graphState.update((current) => ({ ...current, loading: false }));
		}
	},

	/** Replace the working set with an explicit subgraph. */
	async loadSubgraph(ids: string[], projectId?: number) {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({ ...state, projectId: pid, loading: true, error: null }));
		try {
			const response = await graphApi.getSubgraph(pid, ids);
			replaceGraph(response.nodes, response.edges, response.relation_epoch, null);
			return response.nodes.length;
		} catch (error) {
			graphState.update((state) => ({ ...state, loading: false, error: errorMessage(error) }));
			return 0;
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/** Load a bounded slice of the project graph. */
	async loadOverview(limit?: number, projectId?: number) {
		const pid = projectId ?? get(currentProjectId);
		graphState.update((state) => ({ ...state, projectId: pid, loading: true, error: null }));
		try {
			const response = await graphApi.exportGraph(pid, limit);
			replaceGraph(response.nodes, response.edges, response.relation_epoch, null);
			return response.nodes.length;
		} catch (error) {
			graphState.update((state) => ({ ...state, loading: false, error: errorMessage(error) }));
			return 0;
		} finally {
			graphState.update((state) => ({ ...state, loading: false }));
		}
	},

	/**
	 * Fetch connected components and map every node id to its component index.
	 * Components describe communities, which the canvas uses for grouping and
	 * the details panel uses for context.
	 */
	async loadComponents(projectId?: number): Promise<GraphComponentsResponse | null> {
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
				meta: { ...state.meta, communities }
			}));
			return response;
		} catch (error) {
			graphState.update((state) => ({ ...state, error: errorMessage(error) }));
			return null;
		}
	},

	/** Run impact analysis for a changed file and record the result. */
	async loadImpact(file: string, projectId?: number): Promise<GraphImpactResponse | null> {
		const pid = projectId ?? get(currentProjectId);
		try {
			const response = await graphApi.getImpact(pid, file);
			graphState.update((state) => ({
				...state,
				meta: {
					...state.meta,
					impactFile: response.changed_file,
					impactDirect: response.direct_dependents,
					impactTransitive: response.transitive_dependents
				}
			}));
			return response;
		} catch (error) {
			graphState.update((state) => ({ ...state, error: errorMessage(error) }));
			return null;
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
				impactTransitive: []
			}
		}));
	},

	setFilters(patch: Partial<GraphFilters>) {
		graphState.update((state) => ({ ...state, filters: { ...state.filters, ...patch } }));
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
		graphState.update((state) => ({ ...state, filters: { ...state.filters, search } }));
	},

	/** Drop the working set, for example after switching projects. */
	reset(projectId?: number) {
		graphState.set({
			...initialState,
			projectId: projectId ?? get(currentProjectId),
			filters: { ...initialState.filters, domains: [...ALL_DOMAINS] }
		});
	}
};

/** Distinct relation domains present in the current edge set. */
export function activeDomains(edges: GraphEdge[]): Set<RelationDomain> {
	const domains = new Set<RelationDomain>();
	for (const edge of edges) {
		domains.add(relationDomain(edge.relation));
	}
	return domains;
}
