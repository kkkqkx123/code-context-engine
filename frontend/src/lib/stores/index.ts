import { errorMessage } from '../utils/errors';

/**
 * Index State Store
 * Manages indexing operations
 */

import { writable } from 'svelte/store';
import {
	indexApi,
	projectApi,
	type IncrementalIndexRequest,
} from '../api/index';
import { loadProjects } from './project';

export interface IndexState {
	isIndexing: boolean;
	progress: number;
	currentFile: string;
	phase: 'scan' | 'parse' | 'embed' | 'store' | null;
	errorCount: number;
	lastError: string | null;
}

export const indexState = writable<IndexState>({
	isIndexing: false,
	progress: 0,
	currentFile: '',
	phase: null,
	errorCount: 0,
	lastError: null,
});

// Actions
export const indexActions = {
	/**
	 * Index a project from its registered configuration (root path, extensions,
	 * excludes, ignore rules). The project record is the single source of truth
	 * for what gets indexed.
	 */
	async startProjectIndex(projectId: string) {
		indexState.update((state) => ({
			...state,
			isIndexing: true,
			progress: 0,
			phase: 'scan',
		}));

		try {
			await projectApi.indexProject(projectId);
			await loadProjects();
			indexState.update((state) => ({
				...state,
				isIndexing: false,
				progress: 100,
				phase: null,
			}));
		} catch (error) {
			indexState.update((state) => ({
				...state,
				isIndexing: false,
				errorCount: state.errorCount + 1,
				lastError: errorMessage(error),
			}));
		}
	},

	async startIncrementalIndex(data: IncrementalIndexRequest) {
		indexState.update((state) => ({
			...state,
			isIndexing: true,
			progress: 0,
			phase: 'scan',
		}));

		try {
			await indexApi.incrementalIndex(data);
			indexState.update((state) => ({
				...state,
				isIndexing: false,
				progress: 100,
				phase: null,
			}));
		} catch (error) {
			indexState.update((state) => ({
				...state,
				isIndexing: false,
				errorCount: state.errorCount + 1,
				lastError: errorMessage(error),
			}));
		}
	},

	stopIndex() {
		indexState.update((state) => ({
			...state,
			isIndexing: false,
			phase: null,
		}));
	},

	updateProgress(
		progress: number,
		currentFile: string,
		phase: IndexState['phase'],
	) {
		indexState.update((state) => ({
			...state,
			progress,
			currentFile,
			phase,
		}));
	},
};
