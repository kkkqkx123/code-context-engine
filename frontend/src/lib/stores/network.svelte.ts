import { toastStore } from './toast.svelte';

/**
 * Network status store.
 * A rune-based replacement of the previous `writable<boolean>` `isOnline`.
 * The online/offline listeners are wired up once when the singleton is created.
 */
class NetworkStore {
	isOnline = $state(true);

	constructor() {
		if (typeof navigator !== 'undefined') {
			this.isOnline = navigator.onLine;
		}

		if (typeof window !== 'undefined') {
			window.addEventListener('online', () => {
				this.isOnline = true;
				toastStore.success('Connection restored');
			});

			window.addEventListener('offline', () => {
				this.isOnline = false;
				toastStore.warning(
					'You are offline. Some features may be unavailable.',
				);
			});
		}
	}

	/** Check if the application is currently online. */
	checkOnlineStatus(): boolean {
		if (typeof navigator !== 'undefined') {
			return navigator.onLine;
		}
		return true; // Default to online in SSR context
	}
}

export const networkStore = new NetworkStore();
