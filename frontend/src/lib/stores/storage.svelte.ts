/**
 * Storage State Store.
 * Rune-based replacement of the previous `writable<StorageState>` + `storageActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import { storageApi, type StorageStatusDetail } from '../api/storage';
import { indexApi } from '../api/index';
import { projectStore } from './project.svelte';

export interface StorageState {
	status: StorageStatusDetail | null;
	isLoading: boolean;
	error: string | null;
}

class StorageStore {
	status = $state<StorageStatusDetail | null>(null);
	isLoading = $state(false);
	error = $state<string | null>(null);

	async loadStatus() {
		this.isLoading = true;
		this.error = null;

		try {
			const response = await storageApi.getStatus();
			this.status = response.status;
			this.isLoading = false;
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	async clearIndex() {
		this.isLoading = true;
		this.error = null;

		try {
			await indexApi.clearIndex(projectStore.currentProjectId);

			this.isLoading = false;
			await this.loadStatus();
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	clearError() {
		this.error = null;
	}
}

export const storageStore = new StorageStore();
