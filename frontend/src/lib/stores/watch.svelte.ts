/**
 * Watch State Store.
 * Rune-based replacement of the previous `writable<WatchState>` + `watchActions`
 * object. State is `$state` fields; actions are methods on the same instance.
 */
import { errorMessage } from '../utils/errors';
import { watchApi, type WatchStatus } from '../api/watch';
import { projectStore, onProjectChange } from './project.svelte';

export interface WatchState {
	status: WatchStatus | null;
	isWatching: boolean;
	isLoading: boolean;
	error: string | null;
}

let versionPollTimer: ReturnType<typeof setInterval> | null = null;

const VERSION_POLL_MS = 10_000;

class WatchStore {
	status = $state<WatchStatus | null>(null);
	isWatching = $state(false);
	isLoading = $state(false);
	error = $state<string | null>(null);

	async startWatch(path: string, extensions?: string[], debounceMs?: number) {
		this.isLoading = true;
		this.error = null;

		try {
			const pid = projectStore.currentProjectId;

			await watchApi.startWatch(pid, {
				path,
				extensions,
				debounce_ms: debounceMs,
			});
			await this.loadStatus();

			this.isWatching = true;
			this.isLoading = false;
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	async stopWatch() {
		this.isLoading = true;
		this.error = null;

		try {
			const pid = projectStore.currentProjectId;

			await watchApi.stopWatch(pid);
			await this.loadStatus();

			this.isWatching = false;
			this.isLoading = false;
		} catch (error) {
			this.isLoading = false;
			this.error = errorMessage(error);
		}
	}

	async loadStatus() {
		try {
			const pid = projectStore.currentProjectId;

			const response = await watchApi.getStatus(pid);
			this.status = response.status;
			this.isWatching = response.status.active;
		} catch (error) {
			console.error('Failed to load watch status:', error);
		}
	}

	/** Start the single version-epoch polling source; repeated calls are no-ops. */
	startVersionPoll() {
		if (versionPollTimer !== null) return;
		versionPollTimer = setInterval(() => {
			void this.loadStatus();
		}, VERSION_POLL_MS);
	}
}

export const watchStore = new WatchStore();

// Watch status is scoped to one project, so re-read it after a switch.
onProjectChange(() => {
	void watchStore.loadStatus();
});
