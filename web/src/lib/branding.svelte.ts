// Instance branding (white-label, #185) — shared reactive store.
//
// Source of truth is `window.__RUNGU_META__`, injected server-side into the
// SPA shell by `rungud::spa::inject_branding` so a branded instance renders
// correctly from the very first paint (the static shell is baked at build
// time with Rungu defaults). `GET /api/meta` then refreshes the store with
// the license-resolved `poweredBy` flag (see `+layout.svelte` onMount).

export interface InstanceMeta {
    brandName: string;
    logoUrl: string | null;
    footerText: string;
    poweredBy: boolean;
}

export const DEFAULT_META: InstanceMeta = {
    brandName: 'Rungu',
    logoUrl: null,
    footerText: '',
    poweredBy: true,
};

declare global {
    interface Window {
        __RUNGU_META__?: InstanceMeta;
    }
}

/** Read the server-injected boot value, if present and sane. */
export function bootMeta(): Partial<InstanceMeta> {
    if (typeof window === 'undefined') return {};
    const m = window.__RUNGU_META__;
    if (!m || typeof m.brandName !== 'string' || m.brandName.length === 0) return {};
    const out: Partial<InstanceMeta> = { brandName: m.brandName };
    if (typeof m.poweredBy === 'boolean') out.poweredBy = m.poweredBy;
    if (typeof m.footerText === 'string') out.footerText = m.footerText;
    out.logoUrl = typeof m.logoUrl === 'string' ? m.logoUrl : null;
    return out;
}

let current = $state<InstanceMeta>({ ...DEFAULT_META });

/** Apply the boot snapshot. Called once at module init (browser only). */
export function initFromWindow(): void {
    const boot = bootMeta();
    if (Object.keys(boot).length > 0) {
        current = { ...current, ...boot };
    }
}

// Module init: apply the injected meta before any component renders.
// (SSR/prerender: window is undefined → no-op, defaults stay.)
if (typeof window !== 'undefined') {
    initFromWindow();
}

export const branding = {
    get value(): InstanceMeta {
        return current;
    },
    /** Replace the snapshot (called after `GET /api/meta` resolves). */
    set(meta: Partial<InstanceMeta>) {
        current = { ...current, ...meta };
    },
};
