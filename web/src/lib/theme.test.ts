import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
    getStoredTheme,
    resolveTheme,
    cycleTheme,
    applyTheme,
    setTheme,
    persistTheme,
    watchSystemTheme,
    THEME_STORAGE_KEY,
    DARK_CLASS,
    type Theme,
} from '$lib/theme';

// ── jsdom does not implement matchMedia; provide a controllable mock ──

let prefersDark = false;

function installMatchMedia() {
    const listeners = new Set<(e: MediaQueryListEvent) => void>();
    const mql = {
        get matches() {
            return prefersDark;
        },
        media: '(prefers-color-scheme: dark)',
        onchange: null,
        addEventListener: (_ev: string, cb: (e: MediaQueryListEvent) => void) =>
            listeners.add(cb),
        removeEventListener: (_ev: string, cb: (e: MediaQueryListEvent) => void) =>
            listeners.delete(cb),
        addListener: (cb: (e: MediaQueryListEvent) => void) => listeners.add(cb),
        removeListener: (cb: (e: MediaQueryListEvent) => void) => listeners.delete(cb),
        dispatchEvent: () => false,
    };
    vi.stubGlobal('matchMedia', () => mql);
    return {
        mql,
        setPrefersDark(v: boolean) {
            prefersDark = v;
        },
        emit() {
            for (const cb of listeners) {
                cb({ matches: prefersDark } as MediaQueryListEvent);
            }
        },
        clear() {
            listeners.clear();
        },
    };
}

describe('getStoredTheme', () => {
    beforeEach(() => {
        localStorage.clear();
        prefersDark = false;
        installMatchMedia();
    });

    it('defaults to system when nothing is stored', () => {
        expect(getStoredTheme()).toBe('system');
    });

    it('returns a valid stored value', () => {
        localStorage.setItem(THEME_STORAGE_KEY, 'dark');
        expect(getStoredTheme()).toBe('dark');
    });

    it('falls back to system for an invalid value', () => {
        localStorage.setItem(THEME_STORAGE_KEY, 'hot-pink');
        expect(getStoredTheme()).toBe('system');
    });
});

describe('resolveTheme', () => {
    beforeEach(() => {
        prefersDark = false;
        installMatchMedia();
    });

    it('resolves explicit modes directly', () => {
        expect(resolveTheme('light')).toBe('light');
        expect(resolveTheme('dark')).toBe('dark');
    });

    it('resolves system via the OS preference (light)', () => {
        prefersDark = false;
        expect(resolveTheme('system')).toBe('light');
    });

    it('resolves system via the OS preference (dark)', () => {
        prefersDark = true;
        expect(resolveTheme('system')).toBe('dark');
    });
});

describe('cycleTheme', () => {
    it('cycles light → dark → system → light', () => {
        expect(cycleTheme('light')).toBe('dark');
        expect(cycleTheme('dark')).toBe('system');
        expect(cycleTheme('system')).toBe('light');
    });
});

describe('applyTheme / setTheme', () => {
    beforeEach(() => {
        document.documentElement.classList.remove(DARK_CLASS);
        localStorage.clear();
        prefersDark = false;
        installMatchMedia();
    });

    it('adds the dark class for dark', () => {
        applyTheme('dark');
        expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(true);
    });

    it('removes the dark class for light', () => {
        document.documentElement.classList.add(DARK_CLASS);
        applyTheme('light');
        expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(false);
    });

    it('resolves system against the OS preference', () => {
        prefersDark = true;
        applyTheme('system');
        expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(true);

        prefersDark = false;
        applyTheme('system');
        expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(false);
    });

    it('setTheme applies and persists', () => {
        setTheme('dark');
        expect(document.documentElement.classList.contains(DARK_CLASS)).toBe(true);
        expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark');
    });

    it('persistTheme writes to localStorage', () => {
        persistTheme('light');
        expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');
    });
});

describe('watchSystemTheme', () => {
    let mock: ReturnType<typeof installMatchMedia>;

    beforeEach(() => {
        prefersDark = false;
        mock = installMatchMedia();
    });

    afterEach(() => {
        mock.clear();
        vi.unstubAllGlobals();
    });

    it('invokes the callback with the resolved theme on OS change', () => {
        const cb = vi.fn();
        const stop = watchSystemTheme(cb);

        prefersDark = true;
        mock.emit();
        expect(cb).toHaveBeenCalledWith('dark');

        prefersDark = false;
        mock.emit();
        expect(cb).toHaveBeenCalledWith('light');

        stop();
    });

    it('unsubscribe stops further callbacks', () => {
        const cb = vi.fn();
        const stop = watchSystemTheme(cb);
        stop();
        mock.emit();
        expect(cb).not.toHaveBeenCalled();
    });
});

describe('Theme type contract', () => {
    it('exposes the expected storage key and class name', () => {
        expect(THEME_STORAGE_KEY).toBe('rungu-theme');
        expect(DARK_CLASS).toBe('dark');
    });

    it('Theme covers exactly the three modes', () => {
        const themes: Theme[] = ['light', 'dark', 'system'];
        expect(themes).toHaveLength(3);
    });
});
