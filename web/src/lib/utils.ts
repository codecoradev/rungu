import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import type { Snippet } from 'svelte';

// ── Shadcn UI helpers ─────────────────────────────────────────────────

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs));
}

export type WithoutChildren<T> = T extends { children?: Snippet } ? Omit<T, 'children'> : T;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = WithoutChildren<T> & {
    ref?: U | null;
    children?: Snippet;
};

// ── Badge color mapping ───────────────────────────────────────────────

import type { BadgeVariant } from '$lib/components/ui/badge/badge.svelte';

export function statusColor(status: string): BadgeVariant {
    const map: Record<string, BadgeVariant> = {
        open: 'secondary',
        planned: 'default',
        in_progress: 'default',
        done: 'outline',
        declined: 'destructive',
    };
    return map[status] ?? 'outline';
}

// ── Time formatting ───────────────────────────────────────────────────

export function timeAgo(dateStr: string): string {
    if (!dateStr) return '';
    const now = new Date();
    const date = new Date(dateStr);
    const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

    if (seconds < 60) return 'just now';
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days}d ago`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mo ago`;
    return `${Math.floor(months / 12)}y ago`;
}

export function formatDate(dateStr: string): string {
    if (!dateStr) return '';
    return new Date(dateStr).toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
    });
}
