import { writable } from 'svelte/store';
import { toastActions } from './toast';

// Network status store
export const isOnline = writable<boolean>(true);

// Initialize with browser state
if (typeof navigator !== 'undefined') {
	isOnline.set(navigator.onLine);
}

// Listen for online/offline events
if (typeof window !== 'undefined') {
	window.addEventListener('online', () => {
		isOnline.set(true);
		toastActions.success('Connection restored');
	});

	window.addEventListener('offline', () => {
		isOnline.set(false);
		toastActions.warning('You are offline. Some features may be unavailable.');
	});
}

/**
 * Check if the application is currently online
 * @returns boolean indicating online status
 */
export function checkOnlineStatus(): boolean {
	if (typeof navigator !== 'undefined') {
		return navigator.onLine;
	}
	return true; // Default to online in SSR context
}
