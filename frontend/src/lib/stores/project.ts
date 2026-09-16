/**
 * Current Project ID Store
 * Shared writable store for the currently selected project ID.
 * Persisted to localStorage so the selection survives reloads.
 */
import { writable } from 'svelte/store';
import { browser } from '$app/environment';

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

export const currentProjectId = writable<number>(loadInitial());

if (browser) {
	currentProjectId.subscribe((value) => {
		localStorage.setItem(STORAGE_KEY, String(value));
	});
}
