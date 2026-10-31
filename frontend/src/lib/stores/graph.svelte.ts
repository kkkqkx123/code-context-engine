/**
 * Graph State Store.
 *
 * Owns the working set of graph elements shown by the graph explorer, plus the
 * filters applied on top of them. Elements are accumulated incrementally as the
 * user expands neighborhoods, and the server-side relation epoch is tracked so
 * a rebuilt index invalidates the accumulated graph automatically.
 *
 * Rune-based replacement of the previous `writable<GraphState>` + `graphActions`
 * object. State is `$state` fields; actions are methods; the helpers `toGraphError`,
 * `replaceGraph`, `mergeGraph` and `activeDomains` stay at module level (the latter
 * two as private methods operating on the store instance).
 */

import { errorMessage, errorCodeOf } from '../utils/errors';
import {
	graphApi,
	type EntityImpactOptions,
	type GraphComponentsResponse,
	type GraphCyclesResponse,
	type GraphDirection,
	type GraphEdge,
	type GraphFilter,
	type GraphImpactResponse,
	type GraphModuleResponse,
	type GraphNode,
	type GraphPathResponse,
	type GraphStructuralResponse,
	type StructuralOptions,
} from '../api/graph';
import { ApiError, type SymbolCandidate } from '../api/client';
import {
	edgeDomain,
	RELATION_DOMAIN_ORDER,
	type RelationDomain,
} from '../utils/graph-domains';
import {
	edgeElementId,
	toElementEdge,
	toElementNode,
	type GraphElement,
} from '../utils/graph-elements';
import { projectStore } from './project.svelte';
import { SvelteSet } from 'svelte/reactivity';

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
	/** Relation domains left visible; edges in the others are hidden. */
	domains: RelationDomain[];
	kinds: string[];
	search: string;
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

function makeInitialState(): GraphState {
	return {
		projectId: projectStore.currentProjectId,
		nodes: [],
		edges: [],
		elements: [],
		loading: false,
		error: null,
		truncated: false,
		filters: {
			domains: [...RELATION_DOMAIN_ORDER],
			kinds: [],
			search: '',
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
}

function toGraphError(
	error: unknown,
	fallback = 'Graph request failed',
): GraphError {
	// Preserve structured shape from fetch helpers.
	if (error instanceof ApiError) {
		const base: GraphError = {
			code: errorCodeOf(error),
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
			return {
				code: errorCodeOf(error),
				message: errorMessage(error),
				details: { status: obj.status, url: obj.url },
			};
		}
		if ('code' in obj && typeof obj.code === 'string') {
			return { code: obj.code, message: fallback, details: error };
		}
	}
	return { code: 'UNKNOWN', message: fallback, details: error };
}

class GraphStore {
	projectId = $state(makeInitialState().projectId);
	nodes = $state<GraphNode[]>([]);
	edges = $state<GraphEdge[]>([]);
	elements = $state<GraphElement[]>([]);
	loading = $state(false);
	error = $state<GraphError | null>(null);
	truncated = $state(false);
	filters = $state<GraphFilters>(makeInitialState().filters);
	meta = $state<GraphMeta>(makeInitialState().meta);

	/**
	 * Replace the working set. Used when the focus entity changes, or when the
	 * server reports a newer relation epoch than the accumulated data.
	 */
	#replaceGraph(
		nodes: GraphNode[],
		edges: GraphEdge[],
		epoch: number,
		focusId: string | null,
	): void {
		this.nodes = nodes;
		this.edges = edges;
		this.elements = [...nodes.map(toElementNode), ...edges.map(toElementEdge)];
		this.meta = { ...this.meta, epoch, focusId };
		this.error = null;
	}

	/**
	 * Merge a newly fetched neighborhood into the working set.
	 *
	 * Nodes are keyed by id and edges by their derived element id, so expansion is
	 * idempotent: re-fetching a neighborhood the user already expanded adds
	 * nothing. When the response carries a newer epoch the previous data belongs to
	 * a stale index and is discarded instead of merged.
	 */
	#mergeGraph(
		nodes: GraphNode[],
		edges: GraphEdge[],
		epoch: number,
		focusId: string | null,
	): number {
		if (epoch > this.meta.epoch && this.meta.epoch !== 0) {
			this.#replaceGraph(nodes, edges, epoch, focusId);
			return nodes.length;
		}

		const knownNodes = new SvelteSet(this.nodes.map((node) => node.id));
		const knownEdges = new SvelteSet(this.edges.map(edgeElementId));

		const freshNodes = nodes.filter((node) => !knownNodes.has(node.id));
		const freshEdges = edges.filter(
			(edge) => !knownEdges.has(edgeElementId(edge)),
		);

		if (freshNodes.length === 0 && freshEdges.length === 0) {
			return 0;
		}

		this.nodes = [...this.nodes, ...freshNodes];
		this.edges = [...this.edges, ...freshEdges];
		this.elements = [
			...this.elements,
			...freshNodes.map(toElementNode),
			...freshEdges.map(toElementEdge),
		];
		this.meta = {
			...this.meta,
			epoch,
			focusId: focusId ?? this.meta.focusId,
		};

		return freshNodes.length;
	}

	/** Load the neighborhood of an entity and replace the working set. */
	async loadEgo(
		entityId: string,
		depth = 2,
		direction: GraphDirection = 'both',
		projectId?: number,
		filter?: GraphFilter,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? projectStore.currentProjectId;
		this.projectId = pid;
		this.loading = true;
		this.error = null;
		try {
			const response = await graphApi.getEgo(pid, {
				entityId,
				depth,
				direction,
				...filter,
			});
			this.#replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				entityId,
			);
			this.loading = false;
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			this.loading = false;
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/**
	 * Expand the graph by one hop around an entity, merging the result into the
	 * existing working set. Returns the number of newly added nodes.
	 */
	async expand(
		entityId: string,
		depth = 1,
		direction: GraphDirection = 'both',
		filter?: GraphFilter,
	): Promise<GraphActionResult<number>> {
		if (this.nodes.length >= MAX_RENDERED_NODES) {
			this.truncated = true;
			const ge: GraphError = {
				code: 'LIMIT_EXCEEDED',
				message: `Render limit of ${MAX_RENDERED_NODES} nodes reached. Reload a smaller seed to expand further.`,
			};
			this.error = ge;
			return { ok: false, error: ge };
		}
		this.loading = true;
		try {
			const response = await graphApi.getEgo(this.projectId, {
				entityId,
				depth,
				direction,
				...filter,
			});
			const added = this.#mergeGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				entityId,
			);
			this.loading = false;
			return { ok: true, value: added };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			this.loading = false;
			return { ok: false, error: ge };
		}
	}

	/** Replace the working set with an explicit subgraph. */
	async loadSubgraph(
		ids: string[],
		projectId?: number,
		filter?: GraphFilter,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? projectStore.currentProjectId;
		this.projectId = pid;
		this.loading = true;
		this.error = null;
		try {
			const response = await graphApi.getSubgraph(pid, ids, filter);
			this.#replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				null,
			);
			this.loading = false;
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			this.loading = false;
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

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
		filter?: GraphFilter,
	): Promise<GraphActionResult<GraphPathResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		this.projectId = pid;
		this.loading = true;
		this.error = null;
		try {
			const response = await graphApi.getPath(pid, {
				start,
				end,
				maxDepth,
				...filter,
			});
			if (response.path_found) {
				this.#replaceGraph(
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
				this.error = ge;
				return { ok: false, error: ge };
			}
			this.loading = false;
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.loading = false;
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Load a bounded slice of the project graph. */
	async loadOverview(
		limit?: number,
		projectId?: number,
		filter?: GraphFilter,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? projectStore.currentProjectId;
		this.projectId = pid;
		this.loading = true;
		this.error = null;
		try {
			const response = await graphApi.exportGraph(pid, limit, filter);
			this.#replaceGraph(
				response.nodes,
				response.edges,
				response.relation_epoch,
				null,
			);
			this.loading = false;
			return { ok: true, value: response.nodes.length };
		} catch (error) {
			const ge = toGraphError(error);
			this.loading = false;
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/**
	 * Fetch connected components and map every node id to its component index.
	 * Components describe communities, which the canvas uses for grouping and
	 * the details panel uses for context.
	 */
	async loadComponents(
		projectId?: number,
		filter?: GraphFilter,
	): Promise<GraphActionResult<GraphComponentsResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getComponents(pid, filter);
			const communities: Record<string, number> = {};
			response.components.forEach((component, index) => {
				for (const nodeId of component) {
					communities[nodeId] = index;
				}
			});
			this.meta = {
				...this.meta,
				communities,
				communitiesEpoch: response.relation_epoch,
			};
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/**
	 * Fetch connected components only when the relation index has moved past
	 * the epoch the current community data was built from. A no-op when the
	 * graph data is unchanged, so repeated seed loads do not re-fetch the
	 * full component list.
	 */
	async loadComponentsIfStale(
		projectId?: number,
		filter?: GraphFilter,
	): Promise<GraphActionResult<GraphComponentsResponse>> {
		if (this.meta.communitiesEpoch >= this.meta.epoch) {
			return {
				ok: true,
				value: {
					components: [],
					relation_epoch: this.meta.epoch,
					success: true,
				},
			};
		}
		return this.loadComponents(projectId, filter);
	}

	/** Run impact analysis for a changed file and record the result. */
	async loadImpact(
		file: string,
		projectId?: number,
	): Promise<GraphActionResult<GraphImpactResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getImpact(pid, file);
			this.meta = {
				...this.meta,
				impactFile: response.changed_file,
				impactDirect: response.direct_dependents,
				impactTransitive: response.indirect_dependents,
			};
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Run impact analysis for a changed entity. */
	async loadEntityImpact(
		options: EntityImpactOptions,
		projectId?: number,
	): Promise<GraphActionResult<number>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getEntityImpact(pid, options);
			this.meta = {
				...this.meta,
				impactFile: response.changed_entity,
				impactDirect: response.direct_dependents,
				impactTransitive: response.indirect_dependents,
			};
			return { ok: true, value: response.direct_dependents.length };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Fetch dependency cycles at entity or file level. */
	async loadCycles(
		level: 'entity' | 'file' = 'entity',
		limit = 100,
		projectId?: number,
	): Promise<GraphActionResult<GraphCyclesResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getCycles(pid, { level, limit });
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Fetch typed structural relations rooted at one entity. */
	async loadStructural(
		options: StructuralOptions,
		projectId?: number,
	): Promise<GraphActionResult<GraphStructuralResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getStructural(pid, options);
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Fetch module-level relations of one file. */
	async loadModule(
		file: string,
		projectId?: number,
	): Promise<GraphActionResult<GraphModuleResponse>> {
		const pid = projectId ?? projectStore.currentProjectId;
		try {
			const response = await graphApi.getModule(pid, file);
			return { ok: true, value: response };
		} catch (error) {
			const ge = toGraphError(error);
			this.error = ge;
			return { ok: false, error: ge };
		}
	}

	/** Clear the recorded impact analysis highlight. */
	clearImpact() {
		this.meta = {
			...this.meta,
			impactFile: null,
			impactDirect: [],
			impactTransitive: [],
		};
	}

	setFilters(patch: Partial<GraphFilters>) {
		this.filters = { ...this.filters, ...patch };
	}

	toggleDomain(domain: RelationDomain) {
		const active = this.filters.domains.includes(domain)
			? this.filters.domains.filter((item) => item !== domain)
			: [...this.filters.domains, domain];
		this.filters = { ...this.filters, domains: active };
	}

	setSearch(search: string) {
		this.filters = { ...this.filters, search };
	}

	/** Drop the working set, for example after switching projects. */
	reset(projectId?: number) {
		const fresh = makeInitialState();
		this.projectId = projectId ?? fresh.projectId;
		this.nodes = fresh.nodes;
		this.edges = fresh.edges;
		this.elements = fresh.elements;
		this.loading = fresh.loading;
		this.error = fresh.error;
		this.truncated = fresh.truncated;
		this.filters = {
			...fresh.filters,
			domains: [...RELATION_DOMAIN_ORDER],
		};
		this.meta = fresh.meta;
	}
}

export const graphStore = new GraphStore();

/**
 * Distinct relation domains present in the current edge set.
 *
 * Uses the same resolution as the renderer elements so the filter panel's
 * notion of "present" can never disagree with how an edge is actually drawn.
 */
export function activeDomains(edges: GraphEdge[]): Set<RelationDomain> {
	const domains = new SvelteSet<RelationDomain>();
	for (const edge of edges) {
		domains.add(edgeDomain(edge));
	}
	return domains;
}
