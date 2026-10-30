/**
 * Admission Token Store.
 * Client-side credential for admission-enabled remote hosts. The token is
 * kept in localStorage so console sessions survive reloads; it is never sent
 * unless present, keeping local loopback use unchanged.
 *
 * Rune-based replacement of the previous `writable<string>` `apiToken`: the
 * token is a single `$state` field, `hasApiToken` is a derived getter, and the
 * localStorage side effects live in `setApiToken`.
 */
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

class AuthStore {
	token = $state<string>(loadInitial());

	/** True when a non-empty token is present. */
	get hasApiToken(): boolean {
		return this.token.trim().length > 0;
	}

	/** Read the stored token without subscribing; safe on both sides of SSR. */
	readStoredToken(): string {
		if (!browser) return '';
		try {
			return (localStorage.getItem(API_TOKEN_STORAGE_KEY) ?? '').trim();
		} catch {
			return '';
		}
	}

	setApiToken(token: string): void {
		const normalized = token.trim();
		this.token = normalized;
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

	clearApiToken(): void {
		this.setApiToken('');
	}
}

export const authStore = new AuthStore();
