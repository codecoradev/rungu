import { describe, it, expect, beforeEach } from 'vitest';
import {
    REGISTRY,
    resolveShortcut,
    isInputFocused,
    shortcutsByScope,
    normalizeKey,
} from '$lib/shortcuts';

// The helper isn't exported by design — we re-test via resolveShortcut which
// uses it. (normalizeKey is now exported; tested directly below.)

describe('shortcut registry', () => {
    it('every shortcut declares a stable key and human label', () => {
        for (const def of REGISTRY) {
            expect(typeof def.key).toBe('string');
            expect(def.key.length).toBeGreaterThan(0);
            expect(typeof def.label).toBe('string');
            expect(def.label.length).toBeGreaterThan(0);
            expect(['global', 'board', 'post-detail']).toContain(def.scope);
        }
    });

    it('has no duplicate (key, scope) pairs', () => {
        const seen = new Set<string>();
        for (const def of REGISTRY) {
            const k = `${def.key.toLowerCase()}@${def.scope}`;
            expect(seen.has(k), `duplicate ${k}`).toBe(false);
            seen.add(k);
        }
    });

    it('esc and ? are both globally bound', () => {
        const scopes = ['global'] as const;
        const esc = resolveShortcut(new KeyboardEvent('keydown', { key: 'Escape' }), scopes);
        const q = resolveShortcut(new KeyboardEvent('keydown', { key: '?' }), scopes);
        expect(esc?.def.key).toBe('Esc');
        expect(q?.def.key).toBe('?');
    });
});

describe('resolveShortcut', () => {
    it('ignores events with Ctrl/Meta/Alt', () => {
        const scopes = ['global', 'board'] as const;
        const ctrl = new KeyboardEvent('keydown', { key: 'j', ctrlKey: true });
        const meta = new KeyboardEvent('keydown', { key: 'j', metaKey: true });
        const alt = new KeyboardEvent('keydown', { key: 'j', altKey: true });
        expect(resolveShortcut(ctrl, scopes)).toBeUndefined();
        expect(resolveShortcut(meta, scopes)).toBeUndefined();
        expect(resolveShortcut(alt, scopes)).toBeUndefined();
    });

    it('returns undefined for unbound keys', () => {
        const scopes = ['global', 'board'] as const;
        expect(resolveShortcut(new KeyboardEvent('keydown', { key: 'z' }), scopes)).toBeUndefined();
    });

    it('matches board-scoped shortcut when board scope is active', () => {
        const scopes = ['global', 'board'] as const;
        const r = resolveShortcut(new KeyboardEvent('keydown', { key: 'j' }), scopes);
        expect(r?.def.scope).toBe('board');
        expect(r?.def.label).toMatch(/next post/i);
    });

    it('falls back to global when only global is in scope', () => {
        const scopes = ['global'] as const;
        // j is board-scoped; with no board scope, it should not resolve.
        expect(resolveShortcut(new KeyboardEvent('keydown', { key: 'j' }), scopes)).toBeUndefined();
        // ? is global-scoped; should resolve.
        expect(resolveShortcut(new KeyboardEvent('keydown', { key: '?' }), scopes)?.def.key).toBe('?');
    });

    it('prefers the most specific scope when a key is bound in multiple scopes', () => {
        // 'r' is bound in post-detail. With both board + post-detail active,
        // post-detail should win.
        const scopes = ['global', 'board', 'post-detail'] as const;
        const r = resolveShortcut(new KeyboardEvent('keydown', { key: 'r' }), scopes);
        expect(r?.def.scope).toBe('post-detail');
    });

    it('normalizes Escape and Esc to the same key', () => {
        const scopes = ['global'] as const;
        const a = resolveShortcut(new KeyboardEvent('keydown', { key: 'Escape' }), scopes);
        // KeyboardEvent normalizes 'Esc' to 'Escape'; the resolver should still match.
        expect(a?.def.key).toBe('Esc');
    });
});

describe('isInputFocused (document-based)', () => {
    beforeEach(() => {
        // Ensure a clean focus state between tests.
        (document.activeElement as HTMLElement | null)?.blur?.();
    });

    it('returns false when nothing is focused', () => {
        expect(isInputFocused()).toBe(false);
    });

    it('returns true when an input is focused', () => {
        const input = document.createElement('input');
        document.body.appendChild(input);
        input.focus();
        expect(isInputFocused()).toBe(true);
        document.body.removeChild(input);
    });

    it('returns true when a textarea is focused', () => {
        const ta = document.createElement('textarea');
        document.body.appendChild(ta);
        ta.focus();
        expect(isInputFocused()).toBe(true);
        document.body.removeChild(ta);
    });

    // jsdom does not actually move focus to contenteditable elements, so this
    // behavior is exercised in the browser. Kept as a sanity check that the
    // detection code path doesn't throw on a contenteditable node.
    it.skip('returns true when a contenteditable element is focused (browser-only; jsdom does not focus)', () => {
        const div = document.createElement('div');
        div.contentEditable = 'true';
        div.tabIndex = 0;
        document.body.appendChild(div);
        div.focus();
        expect(isInputFocused()).toBe(true);
        document.body.removeChild(div);
    });

    it('returns false when a div without role is focused', () => {
        const div = document.createElement('div');
        document.body.appendChild(div);
        div.focus();
        // Plain divs don't match our input detection unless they have a textbox role.
        expect(isInputFocused()).toBe(false);
        document.body.removeChild(div);
    });
});

describe('shortcutsByScope', () => {
    it('returns three groups with the registry partitioned', () => {
        const groups = shortcutsByScope();
        expect(Object.keys(groups).sort()).toEqual(['board', 'global', 'post-detail']);
        const total = groups.global.length + groups.board.length + groups['post-detail'].length;
        expect(total).toBe(REGISTRY.length);
    });
});

// Sanity: ensure normalizeKey is exported and stable.
describe('normalizeKey', () => {
    it('treats Escape and Esc as equivalent', () => {
        expect(normalizeKey('Escape')).toBe('esc');
        expect(normalizeKey('esc')).toBe('esc');
        expect(normalizeKey('  Escape  ')).toBe('esc');
    });

    it('lowercases and trims other keys', () => {
        expect(normalizeKey('  J  ')).toBe('j');
        expect(normalizeKey('?')).toBe('?');
    });
});
