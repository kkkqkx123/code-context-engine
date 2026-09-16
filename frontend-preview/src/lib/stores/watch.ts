/**
 * Watch State Store
 * Manages file watching operations
 */

import { writable } from 'svelte/store';
import { watchApi, type WatchStatus } from '../api/watch';
import { currentProjectId } from './project';

export interface WatchState {
	status: WatchStatus | null;
	isWatching: boolean;
	events: Array<{
		timestamp: Date;
		eventType: 'create' | 'modify' | 'delete';
		filePath: string;
		action: string;
	}>;
	isLoading: boolean;
	error: string | null;
}

export const watchState = writable<WatchState>({
	status: null,
	isWatching: false,
	events: [],
	isLoading: false,
	error: null,
});

// Actions
export const watchActions = {
	async startWatch(path: string, extensions?: string[], debounceMs?: number) {
		watchState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();

			await watchApi.startWatch(pid!, { path, extensions, debounce_ms: debounceMs });
			await watchActions.loadStatus();

			watchState.update(state => ({
				...state,
				isLoading: false,
				isWatching: true,
			}));
		} catch (error: any) {
			watchState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	async stopWatch() {
		watchState.update(state => ({ ...state, isLoading: true, error: null }));

		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();

			await watchApi.stopWatch(pid!);
			await watchActions.loadStatus();

			watchState.update(state => ({
				...state,
				isLoading: false,
				isWatching: false,
			}));
		} catch (error: any) {
			watchState.update(state => ({
				...state,
				isLoading: false,
				error: error.message,
			}));
		}
	},

	async loadStatus() {
		try {
			let pid: number;
			currentProjectId.subscribe(v => pid = v)();

			const response = await watchApi.getStatus(pid!);
			watchState.update(state => ({
				...state,
				status: response.status,
				isWatching: response.status.active,
			}));
		} catch (error) {
			console.error('Failed to load watch status:', error);
		}
	},

	addEvent(event: Omit<WatchState['events'][0], 'timestamp'>) {
		watchState.update(state => ({
			...state,
			events: [
				{ ...event, timestamp: new Date() },
				...state.events.slice(0, 99), // Keep last 100 events
			],
		}));
	},

	clearEvents() {
		watchState.update(state => ({ ...state, events: [] }));
	},
};