/**
 * Theme system — framework-agnostic, pure TypeScript.
 *
 * Design:
 * - 3 user-facing modes: 'light' | 'dark' | 'system'.
 * - Dark mode is opt-in via a `.dark` class on <html>. This is the universal
 *   Tailwind convention: both the CSS-variable swap (`:root` vs `.dark`) and
 *   the `dark:` utility variant are bound to the SAME class, so they never
 *   desync (the previous `.light`-opt-in approach fought Tailwind's default
 *   media-query `dark:` variant).
 * - All DOM/localStorage access is SSR-safe (guarded behind `browser`-like
 *   checks), so the module can be imported from any Svelte context or a
 *   future non-Svelte layer without blowing up.
 *
 * The Svelte component ({@link ThemeToggle}) is a thin UI shell over this
 * module; the logic lives here so it is reusable and unit-testable.
 */

export type Theme = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

/** localStorage key used for persistence across the app + the FOUC script. */
export const THEME_STORAGE_KEY = 'rungu-theme';

/** The class toggled on <html> to activate dark mode. */
export const DARK_CLASS = 'dark';

const VALID_THEMES: ReadonlySet<Theme> = new Set(['light', 'dark', 'system']);
const CYCLE_ORDER: readonly Theme[] = ['light', 'dark', 'system'];

/** True when running in a browser environment with a DOM. */
function hasDOM(): boolean {
    return typeof document !== 'undefined' && typeof window !== 'undefined';
}

/** Read the OS-level color-scheme preference. SSR-safe (false off-browser). */
export function systemPrefersDark(): boolean {
    if (!hasDOM() || typeof window.matchMedia !== 'function') return false;
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

/** Resolve a (possibly 'system') theme to a concrete light/dark value. */
export function resolveTheme(theme: Theme): ResolvedTheme {
    if (theme === 'system') {
        return systemPrefersDark() ? 'dark' : 'light';
    }
    return theme;
}

/** Read and validate the persisted theme. Defaults to 'system' when unset/invalid. */
export function getStoredTheme(): Theme {
    if (!hasDOM()) return 'system';
    try {
        const stored = localStorage.getItem(THEME_STORAGE_KEY);
        if (stored && VALID_THEMES.has(stored as Theme)) return stored as Theme;
    } catch {
        // localStorage unavailable (incognito, disabled) — fall back to default.
    }
    return 'system';
}

/** Apply a theme to the document by toggling the `.dark` class on <html>. */
export function applyTheme(theme: Theme): void {
    if (!hasDOM()) return;
    const root = document.documentElement;
    const resolved = resolveTheme(theme);
    root.classList.toggle(DARK_CLASS, resolved === 'dark');
}

/** Persist a theme to localStorage. No-op when storage is unavailable. */
export function persistTheme(theme: Theme): void {
    if (!hasDOM()) return;
    try {
        localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
        // localStorage unavailable — keep this session-only.
    }
}

/** Apply + persist a theme in one call. */
export function setTheme(theme: Theme): void {
    applyTheme(theme);
    persistTheme(theme);
}

/** Return the next theme in the light → dark → system → light cycle. */
export function cycleTheme(current: Theme): Theme {
    const idx = CYCLE_ORDER.indexOf(current);
    return CYCLE_ORDER[(idx + 1) % CYCLE_ORDER.length] ?? 'system';
}

/**
 * Watch OS color-scheme changes (only relevant while theme === 'system').
 * The callback receives the newly resolved concrete theme.
 * Returns an unsubscribe function. SSR-safe (returns a no-op off-browser).
 */
export function watchSystemTheme(cb: (resolved: ResolvedTheme) => void): () => void {
    if (!hasDOM() || typeof window.matchMedia !== 'function') return () => {};
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (e: MediaQueryListEvent) => cb(e.matches ? 'dark' : 'light');
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
}
