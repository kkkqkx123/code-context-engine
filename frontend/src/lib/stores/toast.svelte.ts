export interface Toast {
	id: string;
	message: string;
	type: 'success' | 'error' | 'warning' | 'info';
	duration?: number;
}

let toastId = 0;

/**
 * Toast notifications store.
 * A rune-based replacement of the previous `writable<Toast[]>` + `toastActions`
 * object: the list lives in a single `$state` array and the actions are methods
 * on the same instance, so consumers read `toastStore.toasts` directly.
 */
class ToastStore {
	toasts = $state<Toast[]>([]);

	show(message: string, type: Toast['type'] = 'info', duration = 5000) {
		const id = `toast-${++toastId}`;
		const toast: Toast = { id, message, type, duration };
		this.toasts.push(toast);

		if (duration > 0) {
			setTimeout(() => this.dismiss(id), duration);
		}
	}

	dismiss(id: string) {
		this.toasts = this.toasts.filter((toast) => toast.id !== id);
	}

	success(message: string, duration?: number) {
		this.show(message, 'success', duration);
	}

	error(message: string, duration?: number) {
		this.show(message, 'error', duration || 10000);
	}

	warning(message: string, duration?: number) {
		this.show(message, 'warning', duration);
	}

	info(message: string, duration?: number) {
		this.show(message, 'info', duration);
	}
}

export const toastStore = new ToastStore();
