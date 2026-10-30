/**
 * Project State Store.
 * Single source of truth for project identity: the loaded project list, the
 * selected project id, and the selected project derived from both.
 * The selected id is persisted to localStorage so the selection survives reloads.
 *
 * Rune-based replacement of the previous `writable`s + `derived` + `get`:
 * state is `$state` fields, `currentProject` is a derived getter, and
 * `onProjectChange` is implemented with an internal handler list notified by the
 * `currentProjectId` setter (skipping no-op assignments, mirroring the previous
 * "skip initial value" subscription semantics).
 */
import { browser } from '$app/environment';
import { projectApi, type Project } from '../api/index';

const STORAGE_KEY = 'cce.currentProjectId';

function loadInitial(): number {
	if (browser) {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (raw) {
			const parsed = Number(raw);
			if (!Number.isNaN(parsed)) {
				return parsed;
			}
		}
	}
	return 1;
}

class ProjectStore {
	projects = $state<Project[]>([]);
	#currentProjectId = $state(loadInitial());
	#changeHandlers: Array<(projectId: number) => void> = [];

	get currentProjectId(): number {
		return this.#currentProjectId;
	}

	set currentProjectId(projectId: number) {
		if (projectId === this.#currentProjectId) return;
		this.#currentProjectId = projectId;
		if (browser) {
			try {
				localStorage.setItem(STORAGE_KEY, String(projectId));
			} catch {
				// Ignore storage failures; the in-memory value still applies.
			}
		}
		for (const handler of this.#changeHandlers) {
			handler(projectId);
		}
	}

	/**
	 * The selected project, resolved against the loaded list.
	 * Null until the list is loaded or when the selection points at a project
	 * that no longer exists.
	 */
	get currentProject(): Project | null {
		return (
			this.projects.find(
				(project) => Number(project.id) === this.currentProjectId,
			) ?? null
		);
	}

	/**
	 * Load the project list and keep the selection consistent with it, so the
	 * selector can never display one project while requests carry another.
	 */
	async loadProjects() {
		try {
			const response = await projectApi.listProjects();
			this.projects = response.projects;

			const selected = this.currentProjectId;
			const selectionExists = response.projects.some(
				(project) => Number(project.id) === selected,
			);
			if (!selectionExists && response.projects.length > 0) {
				this.currentProjectId = Number(response.projects[0].id);
			}
		} catch (error) {
			console.error('Failed to load projects:', error);
		}
	}

	/**
	 * Invoke `handler` whenever the selected project changes.
	 * The initial value is skipped (via the setter's no-op guard), so a handler
	 * only ever runs for a real switch. Returns the unsubscribe function.
	 */
	onProjectChange(handler: (projectId: number) => void): () => void {
		this.#changeHandlers.push(handler);
		return () => {
			const index = this.#changeHandlers.indexOf(handler);
			if (index !== -1) {
				this.#changeHandlers.splice(index, 1);
			}
		};
	}
}

export const projectStore = new ProjectStore();

/**
 * Standalone convenience wrapper so existing consumers can keep importing
 * `onProjectChange` directly (and so the rune stores `watch`/`search` can
 * register their project-switch handlers at module load). Delegates to the
 * singleton instance.
 */
export function onProjectChange(
	handler: (projectId: number) => void,
): () => void {
	return projectStore.onProjectChange(handler);
}
