/**
 * Admission Token Store
 * Client-side credential for admission-enabled remote hosts. The token is
 * kept in localStorage so console sessions survive reloads; it is never
 * sent unless present, keeping local loopback use unchanged.
 */
import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';

export const API_TOKEN_STORAGE_KEY = 'cce.apiToken';

function loadInitial(): string {
	if (!browser) return '';
	try {
		return localStorage.getItem(API_TOKEN_STORAGE_KEY) ?? '';
	} catch {
		return '';
	}
}

export const apiToken = writable<string>(loadInitial());

export const hasApiToken = derived(apiToken, ($token) => $token.trim().length > 0);

/** Read the stored token without subscribing; safe on both sides of SSR. */
export function readStoredToken(): string {
	if (!browser) return '';
	try {
		return (localStorage.getItem(API_TOKEN_STORAGE_KEY) ?? '').trim();
	} catch {
		return '';
	}
}

export function setApiToken(token: string): void {
	const normalized = token.trim();
	apiToken.set(normalized);
	if (!browser) return;
	try {
		if (normalized) {
			localStorage.setItem(API_TOKEN_STORAGE_KEY, normalized);
		} else {
			localStorage.removeItem(API_TOKEN_STORAGE_KEY);
		}
	} catch {
		// Storage may be unavailable; the in-memory store still applies.
	}
}

export function clearApiToken(): void {
	setApiToken('');
}

if (browser) {
	try {
		const stored = localStorage.getItem(API_TOKEN_STORAGE_KEY) ?? '';
		if (stored.trim()) apiToken.set(stored.trim());
	} catch {
		// Ignore storage failures and keep the in-memory default.
	}
}
