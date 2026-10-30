/**
 * Entity State Store.
 * Rune-based replacement of the previous `writable<EntityState>` + `entityActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import {
	entityApi,
	type FunctionInfo,
	type FunctionCallsResponse,
	type FunctionCallersResponse,
	type ClassInheritanceResponse,
	type ClassImplementationsResponse,
	type ClassificationStatsResponse,
	type RelationFilter,
} from '../api/entities';
import { type CallChainNode } from '../api/search';
import { projectStore } from './project.svelte';

export interface EntityState {
	currentEntity: FunctionInfo | null;
	calls: FunctionCallsResponse | null;
	callers: FunctionCallersResponse | null;
	callChain: CallChainNode[];
	/** Whether the most recent two-point call-path query found a path. */
	callPathFound: boolean;
	/** Chain of nodes on the found call path (empty when none found). */
	callPath: CallChainNode[];
	/** Number of hops on the found call path. */
	callPathLength: number;
	inheritance: ClassInheritanceResponse | null;
	implementations: ClassImplementationsResponse | null;
	classificationStats: ClassificationStatsResponse | null;
	classificationRelations: unknown[];
	classificationCategory: string;
	isLoading: boolean;
	error: string | null;
}

class EntityStore {
	currentEntity = $state<FunctionInfo | null>(null);
	calls = $state<FunctionCallsResponse | null>(null);
	callers = $state<FunctionCallersResponse | null>(null);
	callChain = $state<CallChainNode[]>([]);
	callPathFound = $state(false);
	callPath = $state<CallChainNode[]>([]);
	callPathLength = $state(0);
	inheritance = $state<ClassInheritanceResponse | null>(null);
	implementations = $state<ClassImplementationsResponse | null>(null);
	classificationStats = $state<ClassificationStatsResponse | null>(null);
	classificationRelations = $state<unknown[]>([]);
	classificationCategory = $state('');
	isLoading = $state(false);
	error = $state<string | null>(null);

	async loadFunction(id: string, filter?: RelationFilter) {
		this.isLoading = true;
		this.error = null;

		try {
			const projId = projectStore.currentProjectId;

			const [func, calls, callers] = await Promise.all([
				entityApi.getFunction(projId, id),
				entityApi.getCalls(projId, id, filter),
				entityApi.getCallers(projId, id, filter),
			]);

			this.currentEntity = func.function;
			this.calls = calls;
			this.callers = callers;
			this.isLoading = false;
		} catch (error) {
			console.error('Failed to load function:', error);
			this.isLoading = false;
			this.error = 'Failed to load function details';
		}
	}

	async loadClass(id: string) {
		this.isLoading = true;
		this.error = null;

		try {
			const projId = projectStore.currentProjectId;

			const [inheritance, implementations] = await Promise.all([
				entityApi.getInheritance(projId, id),
				entityApi.getImplementations(projId, id),
			]);

			this.currentEntity = null;
			this.inheritance = inheritance;
			this.implementations = implementations;
			this.isLoading = false;
		} catch (error) {
			console.error('Failed to load class:', error);
			this.isLoading = false;
			this.error = 'Failed to load class details';
		}
	}

	async loadCallChain(
		id: string,
		direction: 'up' | 'down' = 'down',
		filter?: RelationFilter,
	) {
		try {
			const projId = projectStore.currentProjectId;

			const response = await entityApi.getCallChain(
				projId,
				id,
				direction,
				filter,
			);
			this.callChain = response.call_chain;
		} catch (error) {
			console.error('Failed to load call chain:', error);
		}
	}

	/**
	 * Query the shortest call path between two functions. `path_found` is
	 * stored so the UI can distinguish "no path exists" from "request failed";
	 * a previous path is cleared either way.
	 */
	async loadCallPath(fromId: string, toId: string, maxDepth = 10) {
		this.isLoading = true;
		this.error = null;
		this.callPath = [];
		this.callPathFound = false;
		this.callPathLength = 0;

		try {
			const projId = projectStore.currentProjectId;

			const response = await entityApi.getCallPath(
				projId,
				fromId,
				toId,
				maxDepth,
			);
			this.callPathFound = response.path_found;
			this.callPath = response.path ?? [];
			this.callPathLength = response.path_length ?? 0;
			this.isLoading = false;
			this.error = response.path_found
				? null
				: 'No call path found between the given functions';
			return response;
		} catch (error) {
			console.error('Failed to load call path:', error);
			this.isLoading = false;
			this.error = 'Failed to load call path';
			return null;
		}
	}

	clear() {
		this.currentEntity = null;
		this.calls = null;
		this.callers = null;
		this.callChain = [];
		this.callPathFound = false;
		this.callPath = [];
		this.callPathLength = 0;
		this.inheritance = null;
		this.implementations = null;
		this.classificationStats = null;
		this.classificationRelations = [];
		this.classificationCategory = '';
		this.error = null;
	}

	/** Project-level external-call classification counts. */
	async loadClassificationStats() {
		this.isLoading = true;
		this.error = null;
		try {
			const projId = projectStore.currentProjectId;
			const stats = await entityApi.getClassificationStats(projId);
			this.classificationStats = stats;
			this.isLoading = false;
			return stats;
		} catch (error) {
			console.error('Failed to load classification stats:', error);
			this.isLoading = false;
			this.error = 'Failed to load classification stats';
			return null;
		}
	}

	/** Relations in one external-call classification category. */
	async loadRelationsByClassification(classification: string, limit?: number) {
		this.isLoading = true;
		this.error = null;
		this.classificationCategory = classification;
		try {
			const projId = projectStore.currentProjectId;
			const relations = await entityApi.getRelationsByClassification(
				projId,
				classification,
				limit,
			);
			this.classificationRelations = relations;
			this.isLoading = false;
			return relations;
		} catch (error) {
			console.error('Failed to load classification relations:', error);
			this.isLoading = false;
			this.error = 'Failed to load classification relations';
			return null;
		}
	}
}

export const entityStore = new EntityStore();
