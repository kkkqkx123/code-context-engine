/**
 * Entity State Store
 * Manages entity details and relationship exploration
 */

import { writable } from 'svelte/store';
import { entityApi, type FunctionInfo, type FunctionCallsResponse, type FunctionCallersResponse, type ClassInheritanceResponse, type ClassImplementationsResponse } from '../api/entities';
import { type CallChainNode } from '../api/search';
import { currentProjectId } from './project';

export interface EntityState {
	currentEntity: FunctionInfo | null;
	calls: FunctionCallsResponse | null;
	callers: FunctionCallersResponse | null;
	callChain: CallChainNode[];
	inheritance: ClassInheritanceResponse | null;
	implementations: ClassImplementationsResponse | null;
	isLoading: boolean;
	error: string | null;
}

export const entityState = writable<EntityState>({
	currentEntity: null,
	calls: null,
	callers: null,
	callChain: [],
	inheritance: null,
	implementations: null,
	isLoading: false,
	error: null,
});

// Actions
export const entityActions = {
	async loadFunction(id: string, projectId?: number) {
		entityState.update(s => ({ ...s, isLoading: true, error: null }));

		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();
			const projId = projectId ?? pid!;

			const [func, calls, callers] = await Promise.all([
				entityApi.getFunction(projId, id),
				entityApi.getCalls(projId, id),
				entityApi.getCallers(projId, id),
			]);

			entityState.update(s => ({
				...s,
				currentEntity: func.function,
				calls,
				callers,
				isLoading: false,
			}));
		} catch (error) {
			console.error('Failed to load function:', error);
			entityState.update(s => ({
				...s,
				isLoading: false,
				error: 'Failed to load function details',
			}));
		}
	},

	async loadClass(id: string, projectId?: number) {
		entityState.update(s => ({ ...s, isLoading: true, error: null }));

		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();
			const projId = projectId ?? pid!;

			const [inheritance, implementations] = await Promise.all([
				entityApi.getInheritance(projId, id),
				entityApi.getImplementations(projId, id),
			]);

			entityState.update(s => ({
				...s,
				currentEntity: null,
				inheritance,
				implementations,
				isLoading: false,
			}));
		} catch (error) {
			console.error('Failed to load class:', error);
			entityState.update(s => ({
				...s,
				isLoading: false,
				error: 'Failed to load class details',
			}));
		}
	},

	async loadCallChain(id: string, direction: 'up' | 'down' = 'down', projectId?: number) {
		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();
			const projId = projectId ?? pid!;

			const response = await entityApi.getCallChain(projId, id, direction);
			entityState.update(s => ({ ...s, callChain: response.call_chain }));
		} catch (error) {
			console.error('Failed to load call chain:', error);
		}
	},

	clear() {
		entityState.update(s => ({
			...s,
			currentEntity: null,
			calls: null,
			callers: null,
			callChain: [],
			inheritance: null,
			implementations: null,
			error: null,
		}));
	},
};