/**
 * Index State Store
 * Manages indexing operations and project state
 */

import { writable, derived, get } from 'svelte/store';
import { indexApi, projectApi, type Project } from '../api/index';
import { currentProjectId } from './project';

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

// Projects store
export const projects = writable<Project[]>([]);
export const selectedProject = writable<Project | null>(null);

// Load projects on initialization
export async function loadProjects() {
	try {
		const response = await projectApi.listProjects();
		projects.set(response.projects);
	} catch (error) {
		console.error('Failed to load projects:', error);
	}
}

// Derived store for active projects
export const activeProjects = derived(projects, $projects =>
	$projects.filter(p => p.id)
);

// Actions
export const indexActions = {
	async startIndex(data: any) {
		indexState.update(state => ({
			...state,
			isIndexing: true,
			progress: 0,
			phase: 'scan',
		}));

		try {
			await indexApi.runIndex(data);
			indexState.update(state => ({
				...state,
				isIndexing: false,
				progress: 100,
				phase: null,
			}));
		} catch (error: any) {
			indexState.update(state => ({
				...state,
				isIndexing: false,
				errorCount: state.errorCount + 1,
				lastError: error.message,
			}));
		}
	},

	async startIncrementalIndex(data: any) {
		indexState.update(state => ({
			...state,
			isIndexing: true,
			progress: 0,
			phase: 'scan',
		}));

		try {
			const pid = get(currentProjectId);
			await indexApi.incrementalIndex({ ...data, project_id: pid });
			indexState.update(state => ({
				...state,
				isIndexing: false,
				progress: 100,
				phase: null,
			}));
		} catch (error: any) {
			indexState.update(state => ({
				...state,
				isIndexing: false,
				errorCount: state.errorCount + 1,
				lastError: error.message,
			}));
		}
	},

	stopIndex() {
		indexState.update(state => ({
			...state,
			isIndexing: false,
			phase: null,
		}));
	},

	updateProgress(progress: number, currentFile: string, phase: IndexState['phase']) {
		indexState.update(state => ({
			...state,
			progress,
			currentFile,
			phase,
		}));
	},
};
