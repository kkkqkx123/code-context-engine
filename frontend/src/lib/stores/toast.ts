import { writable } from 'svelte/store';

export interface Toast {
	id: string;
	message: string;
	type: 'success' | 'error' | 'warning' | 'info';
	duration?: number;
}

export const toasts = writable<Toast[]>([]);

let toastId = 0;

export const toastActions = {
	show(message: string, type: Toast['type'] = 'info', duration = 5000) {
		const id = `toast-${++toastId}`;
		const toast: Toast = { id, message, type, duration };
		
		toasts.update(state => [...state, toast]);
		
		if (duration > 0) {
			setTimeout(() => {
				this.dismiss(id);
			}, duration);
		}
	},
	
	dismiss(id: string) {
		toasts.update(state => state.filter(t => t.id !== id));
	},
	
	success(message: string, duration?: number) {
		this.show(message, 'success', duration);
	},
	
	error(message: string, duration?: number) {
		this.show(message, 'error', duration || 10000);
	},
	
	warning(message: string, duration?: number) {
		this.show(message, 'warning', duration);
	},
	
	info(message: string, duration?: number) {
		this.show(message, 'info', duration);
	}
};
