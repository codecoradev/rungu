import { writable } from 'svelte/store';

export type ToastKind = 'success' | 'error';

export interface Toast {
    id: number;
    kind: ToastKind;
    message: string;
    /** Optional action rendered as a link (e.g. "Login →"). */
    action?: { label: string; href: string };
}

let nextId = 0;
export const toasts = writable<Toast[]>([]);

/**
 * Show a transient toast. Errors use role=alert (assertive),
 * success uses role=status (polite) — WCAG 4.1.3 / #144.
 */
function push(kind: ToastKind, message: string, opts?: { action?: Toast['action'] }) {
    const id = ++nextId;
    toasts.update((list) => [...list, { id, kind, message, action: opts?.action }]);
    const timeout = kind === 'error' ? 5000 : 2500;
    setTimeout(() => dismiss(id), timeout);
}

export function toastSuccess(message: string) {
    push('success', message);
}

export function toastError(message: string, opts?: { action?: Toast['action'] }) {
    push('error', message, opts);
}

export function dismiss(id: number) {
    toasts.update((list) => list.filter((t) => t.id !== id));
}
