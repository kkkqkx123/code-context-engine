/**
 * Storage State Store
 * Manages storage operations and status
 */

import { writable } from 'svelte/store';
import { storageApi, type StorageStatusDetail } from '../api/storage';
import { indexApi } from '../api/index';
import { currentProjectId } from './project';

export interface StorageState {
	status: StorageStatusDetail | null;
	isLoading: boolean;
	error: string | null;
}

export const storageState = writable<StorageState>({
	status: null,
	isLoading: false,
	error: null,
});

// Actions
export const storageActions = {
	async loadStatus() {
		storageState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			const response = await storageApi.getStatus();
			storageState.update(state => ({
				...state,
				status: response.status,
				isLoading: false,
			}));
		} catch (error: any) {
			storageState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	async clearIndex() {
		storageState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();

			await indexApi.clearIndex(pid!);

			storageState.update(state => ({
				...state,
				isLoading: false,
			}));

			await storageActions.loadStatus();
		} catch (error: any) {
			storageState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	clearError() {
		storageState.update(state => ({ ...state, error: null }));
	},
};