/**
 * Entity State Store
 * Manages entity details and relationship exploration
 */

import { writable, get } from 'svelte/store';
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
import { currentProjectId } from './project';

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

export const entityState = writable<EntityState>({
	currentEntity: null,
	calls: null,
	callers: null,
	callChain: [],
	callPathFound: false,
	callPath: [],
	callPathLength: 0,
	inheritance: null,
	implementations: null,
	classificationStats: null,
	classificationRelations: [],
	classificationCategory: '',
	isLoading: false,
	error: null,
});

// Actions
export const entityActions = {
	async loadFunction(id: string, filter?: RelationFilter) {
		entityState.update((s) => ({ ...s, isLoading: true, error: null }));

		try {
			const projId = get(currentProjectId);

			const [func, calls, callers] = await Promise.all([
				entityApi.getFunction(projId, id),
				entityApi.getCalls(projId, id, filter),
				entityApi.getCallers(projId, id, filter),
			]);

			entityState.update((s) => ({
				...s,
				currentEntity: func.function,
				calls,
				callers,
				isLoading: false,
			}));
		} catch (error) {
			console.error('Failed to load function:', error);
			entityState.update((s) => ({
				...s,
				isLoading: false,
				error: 'Failed to load function details',
			}));
		}
	},

	async loadClass(id: string) {
		entityState.update((s) => ({ ...s, isLoading: true, error: null }));

		try {
			const projId = get(currentProjectId);

			const [inheritance, implementations] = await Promise.all([
				entityApi.getInheritance(projId, id),
				entityApi.getImplementations(projId, id),
			]);

			entityState.update((s) => ({
				...s,
				currentEntity: null,
				inheritance,
				implementations,
				isLoading: false,
			}));
		} catch (error) {
			console.error('Failed to load class:', error);
			entityState.update((s) => ({
				...s,
				isLoading: false,
				error: 'Failed to load class details',
			}));
		}
	},

	async loadCallChain(
		id: string,
		direction: 'up' | 'down' = 'down',
		filter?: RelationFilter,
	) {
		try {
			const projId = get(currentProjectId);

			const response = await entityApi.getCallChain(
				projId,
				id,
				direction,
				filter,
			);
			entityState.update((s) => ({ ...s, callChain: response.call_chain }));
		} catch (error) {
			console.error('Failed to load call chain:', error);
		}
	},

	/**
	 * Query the shortest call path between two functions. `path_found` is
	 * stored so the UI can distinguish "no path exists" from "request failed";
	 * a previous path is cleared either way.
	 */
	async loadCallPath(fromId: string, toId: string, maxDepth = 10) {
		entityState.update((s) => ({
			...s,
			isLoading: true,
			error: null,
			callPath: [],
			callPathFound: false,
			callPathLength: 0,
		}));

		try {
			const projId = get(currentProjectId);

			const response = await entityApi.getCallPath(
				projId,
				fromId,
				toId,
				maxDepth,
			);
			entityState.update((s) => ({
				...s,
				callPathFound: response.path_found,
				callPath: response.path ?? [],
				callPathLength: response.path_length ?? 0,
				isLoading: false,
				error: response.path_found
					? null
					: 'No call path found between the given functions',
			}));
			return response;
		} catch (error) {
			console.error('Failed to load call path:', error);
			entityState.update((s) => ({
				...s,
				isLoading: false,
				error: 'Failed to load call path',
			}));
			return null;
		}
	},

	clear() {
		entityState.update((s) => ({
			...s,
			currentEntity: null,
			calls: null,
			callers: null,
			callChain: [],
			callPathFound: false,
			callPath: [],
			callPathLength: 0,
			inheritance: null,
			implementations: null,
			classificationStats: null,
			classificationRelations: [],
			classificationCategory: '',
			error: null,
		}));
	},

	/** Project-level external-call classification counts. */
	async loadClassificationStats() {
		entityState.update((s) => ({ ...s, isLoading: true, error: null }));
		try {
			const projId = get(currentProjectId);
			const stats = await entityApi.getClassificationStats(projId);
			entityState.update((s) => ({
				...s,
				classificationStats: stats,
				isLoading: false,
			}));
			return stats;
		} catch (error) {
			console.error('Failed to load classification stats:', error);
			entityState.update((s) => ({
				...s,
				isLoading: false,
				error: 'Failed to load classification stats',
			}));
			return null;
		}
	},

	/** Relations in one external-call classification category. */
	async loadRelationsByClassification(classification: string, limit?: number) {
		entityState.update((s) => ({
			...s,
			isLoading: true,
			error: null,
			classificationCategory: classification,
		}));
		try {
			const projId = get(currentProjectId);
			const relations = await entityApi.getRelationsByClassification(
				projId,
				classification,
				limit,
			);
			entityState.update((s) => ({
				...s,
				classificationRelations: relations,
				isLoading: false,
			}));
			return relations;
		} catch (error) {
			console.error('Failed to load classification relations:', error);
			entityState.update((s) => ({
				...s,
				isLoading: false,
				error: 'Failed to load classification relations',
			}));
			return null;
		}
	},
};
