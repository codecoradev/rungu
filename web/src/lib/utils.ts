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
    const timestamp = new Date(dateStr).getTime();
    if (Number.isNaN(timestamp)) return '';

    const now = Date.now();
    const seconds = Math.floor((now - timestamp) / 1000);

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
    const timestamp = new Date(dateStr).getTime();
    if (Number.isNaN(timestamp)) return '';
    return new Date(timestamp).toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
    });
}

// ── URL sanitization ─────────────────────────────────────────────────

const SAFE_URL_PROTOCOLS = new Set(['http:', 'https:', 'mailto:', 'tel:']);

/**
 * Sanitize a potentially user-supplied href.
 *
 * Allows absolute URLs with a safe protocol (http/https/mailto/tel) and any
 * relative URL (path, query, hash). Rejects everything else — including
 * `javascript:`, `data:`, `vbscript:`, and protocol-relative `//evil.com` —
 * by returning `undefined` so the caller can render a `<span>` instead of an
 * `<a>`.
 *
 * Defense-in-depth: even though no current caller passes untrusted data to
 * the badge/button components, this lives in the shared UI kit and could be
 * abused by future code.
 */
export function sanitizeHref(href: string | undefined | null): string | undefined {
    if (href == null || href === '') return undefined;

    const trimmed = href.trim();
    if (trimmed === '') return undefined;

    // Relative URLs are always safe (path, query, hash).
    // Note: protocol-relative `//host` is NOT relative — it inherits the
    // document protocol — so we reject it explicitly.
    if (trimmed.startsWith('/') && !trimmed.startsWith('//')) {
        return trimmed;
    }
    if (trimmed.startsWith('#') || trimmed.startsWith('?')) {
        return trimmed;
    }

    // Absolute URL — validate protocol via the URL parser.
    try {
        const url = new URL(trimmed);
        if (SAFE_URL_PROTOCOLS.has(url.protocol)) {
            return trimmed;
        }
    } catch {
        // Not a parseable absolute URL — reject defensively.
    }
    return undefined;
}
