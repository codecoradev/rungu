import { describe, it, expect } from 'vitest';
import { cn, timeAgo, formatDate, sanitizeHref } from '$lib/utils';

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

    it('returns empty for invalid date strings (no "NaNy ago")', () => {
        expect(timeAgo('not-a-date')).toBe('');
        expect(timeAgo('2026-13-99')).toBe('');
        // Regression: previously returned "NaNy ago" because date.getTime() is NaN.
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

    it('returns empty for invalid date strings (no "Invalid Date" text)', () => {
        expect(formatDate('not-a-date')).toBe('');
        expect(formatDate('garbage')).toBe('');
        // Regression: previously returned locale-dependent "Invalid Date".
    });
});

describe('sanitizeHref', () => {
    it('returns undefined for empty / null / undefined', () => {
        expect(sanitizeHref(undefined)).toBeUndefined();
        expect(sanitizeHref(null)).toBeUndefined();
        expect(sanitizeHref('')).toBeUndefined();
        expect(sanitizeHref('   ')).toBeUndefined();
    });

    it('passes through safe relative URLs', () => {
        expect(sanitizeHref('/')).toBe('/');
        expect(sanitizeHref('/board/my-app')).toBe('/board/my-app');
        expect(sanitizeHref('#section')).toBe('#section');
        expect(sanitizeHref('?q=1')).toBe('?q=1');
    });

    it('passes through safe absolute protocols', () => {
        expect(sanitizeHref('https://example.com')).toBe('https://example.com');
        expect(sanitizeHref('http://example.com')).toBe('http://example.com');
        expect(sanitizeHref('mailto:foo@example.com')).toBe('mailto:foo@example.com');
        expect(sanitizeHref('tel:+15551234')).toBe('tel:+15551234');
    });

    it('rejects protocol-relative URLs (open-redirect vector)', () => {
        expect(sanitizeHref('//evil.com')).toBeUndefined();
        expect(sanitizeHref(' //evil.com')).toBeUndefined();
    });

    it('rejects dangerous schemes (XSS vectors)', () => {
        expect(sanitizeHref('javascript:alert(1)')).toBeUndefined();
        expect(sanitizeHref('JaVaScRiPt:alert(1)')).toBeUndefined();
        expect(sanitizeHref('data:text/html,<script>alert(1)</script>')).toBeUndefined();
        expect(sanitizeHref('vbscript:msgbox')).toBeUndefined();
    });

    it('rejects non-URL garbage', () => {
        expect(sanitizeHref('not a url at all')).toBeUndefined();
    });
});
