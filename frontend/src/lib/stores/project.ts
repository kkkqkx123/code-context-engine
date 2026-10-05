/**
 * Project State Store
 * Single source of truth for project identity: the loaded project list, the
 * selected project id, and the selected project derived from both.
 * The selected id is persisted to localStorage so the selection survives reloads.
 */
import { writable, derived, get } from 'svelte/store';
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

export const projects = writable<Project[]>([]);

export const currentProjectId = writable<number>(loadInitial());

/**
 * The selected project, resolved against the loaded list.
 * Null until the list is loaded or when the selection points at a project
 * that no longer exists.
 */
export const currentProject = derived(
	[projects, currentProjectId],
	([$projects, $currentProjectId]) =>
		$projects.find((project) => Number(project.id) === $currentProjectId) ??
		null,
);

/**
 * Load the project list and keep the selection consistent with it, so the
 * selector can never display one project while requests carry another.
 */
export async function loadProjects() {
	try {
		const response = await projectApi.listProjects();
		projects.set(response.projects);

		const selected = get(currentProjectId);
		const selectionExists = response.projects.some(
			(project) => Number(project.id) === selected,
		);
		if (!selectionExists && response.projects.length > 0) {
			currentProjectId.set(Number(response.projects[0].id));
		}
	} catch (error) {
		console.error('Failed to load projects:', error);
	}
}

/**
 * Invoke `handler` whenever the selected project changes.
 * The initial value delivered by the store subscription is skipped, so a
 * handler only ever runs for a real switch. Returns the unsubscribe function,
 * which makes it usable directly as an `$effect` cleanup.
 */
export function onProjectChange(handler: (projectId: number) => void) {
	let last = get(currentProjectId);
	return currentProjectId.subscribe((projectId) => {
		if (projectId === last) return;
		last = projectId;
		handler(projectId);
	});
}

if (browser) {
	currentProjectId.subscribe((value) => {
		localStorage.setItem(STORAGE_KEY, String(value));
	});
}
