import type { Toast, ToastType } from '$lib/types';

let nextId = 0;
let toasts = $state<Toast[]>([]);

export function getToasts() {
	return toasts;
}

export function addToast(type: ToastType, message: string, duration = 3500) {
	const id = ++nextId;
	if (toasts.length >= 5) {
		toasts = toasts.slice(1);
	}
	toasts = [...toasts, { id, type, message }];
	setTimeout(() => removeToast(id), duration);
}

export function removeToast(id: number) {
	toasts = toasts.filter((t) => t.id !== id);
}
