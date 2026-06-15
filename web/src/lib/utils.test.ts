import { describe, it, expect } from 'vitest';
import { cn, timeAgo, formatDate } from '$lib/utils';

describe('cn utility', () => {
    it('merges class names', () => {
        expect(cn('foo', 'bar')).toBe('foo bar');
    });

    it('handles conditional classes', () => {
        expect(cn('base', false && 'hidden', 'active')).toBe('base active');
    });

    it('deduplicates tailwind classes', () => {
        expect(cn('px-2', 'px-4')).toBe('px-4');
    });
});

describe('timeAgo', () => {
    it('returns "just now" for recent dates', () => {
        const now = new Date().toISOString();
        expect(timeAgo(now)).toBe('just now');
    });

    it('returns minutes format', () => {
        const date = new Date(Date.now() - 5 * 60 * 1000).toISOString();
        expect(timeAgo(date)).toBe('5m ago');
    });

    it('returns hours format', () => {
        const date = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
        expect(timeAgo(date)).toBe('3h ago');
    });

    it('returns days format', () => {
        const date = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString();
        expect(timeAgo(date)).toBe('7d ago');
    });

    it('returns empty for empty string', () => {
        expect(timeAgo('')).toBe('');
    });
});

describe('formatDate', () => {
    it('formats ISO date string', () => {
        const result = formatDate('2026-01-15T10:00:00Z');
        expect(result).toContain('2026');
        expect(result).toContain('Jan');
    });

    it('returns empty for empty string', () => {
        expect(formatDate('')).toBe('');
    });
});
