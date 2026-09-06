import '@testing-library/jest-dom/vitest';

// jsdom (as configured) may not expose a functional localStorage in every
// vitest run — provide a minimal in-memory stub when it's missing so tests
// that touch persistence (theme, branding) have a working storage.
if (typeof globalThis.localStorage === 'undefined') {
    const store = new Map<string, string>();
    const stub: Storage = {
        get length() {
            return store.size;
        },
        clear: () => store.clear(),
        getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
        key: (i: number) => Array.from(store.keys())[i] ?? null,
        removeItem: (k: string) => void store.delete(k),
        setItem: (k: string, v: string) => void store.set(k, String(v)),
    };
    Object.defineProperty(globalThis, 'localStorage', { value: stub, configurable: true });
    Object.defineProperty(globalThis.window ||= globalThis as unknown as Window & typeof globalThis, 'localStorage', {
        value: stub,
        configurable: true,
    });
}
