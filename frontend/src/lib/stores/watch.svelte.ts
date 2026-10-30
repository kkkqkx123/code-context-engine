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
	// Local event feed is intentionally empty: there is no streaming channel.
	// The server-side `events_processed` counter in `status` is the source of
	// truth and is refreshed by `loadStatus` polling.
	events: Array<{
		timestamp: Date;
		eventType: 'create' | 'modify' | 'delete';
		filePath: string;
		action: string;
	}>;
	isLoading: boolean;
	error: string | null;
}

let versionPollTimer: ReturnType<typeof setInterval> | null = null;

const VERSION_POLL_MS = 10_000;

class WatchStore {
	status = $state<WatchStatus | null>(null);
	isWatching = $state(false);
	events = $state<
		Array<{
			timestamp: Date;
			eventType: 'create' | 'modify' | 'delete';
			filePath: string;
			action: string;
		}>
	>([]);
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

	startVersionPoll() {
		if (versionPollTimer !== null) return;
		versionPollTimer = setInterval(() => {
			void this.loadStatus();
		}, VERSION_POLL_MS);
	}

	stopVersionPoll() {
		if (versionPollTimer !== null) {
			clearInterval(versionPollTimer);
			versionPollTimer = null;
		}
	}

	clearEvents() {
		this.events = [];
	}
}

export const watchStore = new WatchStore();

// Watch status is scoped to one project, so re-read it after a switch.
onProjectChange(() => {
	void watchStore.loadStatus();
});
