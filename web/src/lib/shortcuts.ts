// Keyboard shortcut registry + helpers.
//
// Design:
// - Single source of truth for all shortcut bindings (this file).
// - Per-route pages opt in via `use:shortcutBinding` or by dispatching
//   events themselves; the global handler is wired in +layout.svelte.
// - Shortcuts are suppressed when an input/textarea/[contenteditable] has
//   focus (except Escape, which always fires so modals/close-on-esc work).
// - The help overlay (?) is auto-generated from REGISTRY so docs can't drift.
//
// Accessibility:
// - Every shortcut must ALSO be reachable via a visible control (button/link).
// - Shortcuts supplement, never replace, the visible UI.
// - Don't bind keys that conflict with screen-reader / browser shortcuts
//   (avoid Ctrl/Meta combos, avoid keys that AT tools rely on when unfocused).

export interface ShortcutDef {
    /** Display key, e.g. "j", "/", "?", "Esc". Case-insensitive on lookup. */
    key: string;
    /** Human label for the help overlay, e.g. "Next post". */
    label: string;
    /** Where the shortcut is available — drives grouping in the help overlay. */
    scope: 'global' | 'board' | 'post-detail';
    /** If true, the shortcut also requires auth (gated client-side). */
    authRequired?: boolean;
    /** Optional short help text beyond the label. */
    hint?: string;
}

/** Ordered registry. The help overlay renders in this order within each scope. */
export const REGISTRY: readonly ShortcutDef[] = [
    // Global
    { key: '?', label: 'Open this help', scope: 'global', hint: 'Press ? again or Esc to close' },
    { key: 'Esc', label: 'Close modal / clear focus', scope: 'global' },

    // Board
    { key: '/', label: 'Focus search', scope: 'board' },
    { key: 'c', label: 'Compose new post', scope: 'board', authRequired: true },
    { key: 'j', label: 'Next post', scope: 'board' },
    { key: 'k', label: 'Previous post', scope: 'board' },
    { key: 'Enter', label: 'Open focused post', scope: 'board' },
    { key: 'v', label: 'Vote on focused post', scope: 'board', authRequired: true },

    // Post detail
    { key: 'r', label: 'Focus reply box', scope: 'post-detail' },
] as const;

/** Lookup table: normalized key → matching definitions (a key may be valid in multiple scopes). */
const KEY_INDEX: Map<string, ShortcutDef[]> = (() => {
    const m = new Map<string, ShortcutDef[]>();
    for (const def of REGISTRY) {
        const k = normalizeKey(def.key);
        const arr = m.get(k) ?? [];
        arr.push(def);
        m.set(k, arr);
    }
    return m;
})();

/** Normalize a key for matching: lowercase, trim. Handles "Esc" vs "Escape". */
export function normalizeKey(k: string): string {
    const lower = k.trim().toLowerCase();
    // Treat "escape" and "esc" the same.
    if (lower === 'escape') return 'esc';
    return lower;
}

/** Returns true when the currently focused element is a text-input-like field. */
export function isInputFocused(): boolean {
    if (typeof document === 'undefined') return false;
    const el = document.activeElement;
    if (!el) return false;
    const tag = el.tagName.toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
    if ((el as HTMLElement).isContentEditable) return true;
    // Svelte components sometimes wrap inputs in a div with role="textbox".
    if (el.getAttribute('role') === 'textbox') return true;
    return false;
}

export interface ResolvedShortcut {
    def: ShortcutDef;
}

/**
 * Resolve a keyboard event against the registry for the active scope(s).
 *
 * @param event  The KeyboardEvent.
 * @param scopes Active scopes (e.g. ['global', 'board']). Most specific scope wins.
 * @returns The matching definition, or `undefined` if no shortcut applies.
 *
 * Notes:
 * - When an input has focus, returns `undefined` unless the key resolves to
 *   the global Escape binding (so Esc still closes modals from within a field).
 * - Ignores events involving Ctrl/Meta/Alt to avoid clobbering browser/AT
 *   shortcuts (e.g. Ctrl+c copy, Cmd+k).
 */
export function resolveShortcut(
    event: KeyboardEvent,
    scopes: readonly ShortcutDef['scope'][],
): ResolvedShortcut | undefined {
    // Ignore modifier combos — we only own bare-key shortcuts.
    if (event.ctrlKey || event.metaKey || event.altKey) return undefined;

    const key = normalizeKey(event.key);
    const matches = KEY_INDEX.get(key);
    if (!matches || matches.length === 0) return undefined;

    const inputFocused = isInputFocused();
    if (inputFocused && key !== 'esc') return undefined;

    // Prefer the most specific scope. Order: post-detail > board > global.
    const priority: Record<ShortcutDef['scope'], number> = {
        'post-detail': 3,
        board: 2,
        global: 1,
    };
    const scoped = scopes.slice().sort((a, b) => priority[b] - priority[a]);
    for (const scope of scoped) {
        const def = matches.find((m) => m.scope === scope);
        if (def) return { def };
    }
    return undefined;
}

/** Group the registry by scope for help-overlay rendering. */
export function shortcutsByScope(): Record<ShortcutDef['scope'], ShortcutDef[]> {
    const out: Record<ShortcutDef['scope'], ShortcutDef[]> = {
        global: [],
        board: [],
        'post-detail': [],
    };
    for (const def of REGISTRY) out[def.scope].push(def);
    return out;
}
