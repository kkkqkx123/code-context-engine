/**
 * Current Project ID Store
 * Shared writable store for the currently selected project ID
 */
import { writable } from 'svelte/store';

export const currentProjectId = writable<number>(1);