import { describe, expect, it, beforeEach } from 'vitest';
import { branding, DEFAULT_META, bootMeta, initFromWindow, type InstanceMeta } from './branding.svelte';

describe('branding store', () => {
    beforeEach(() => {
        branding.set({ ...DEFAULT_META });
        delete window.__RUNGU_META__;
    });

    it('starts with Rungu defaults', () => {
        expect(branding.value.brandName).toBe('Rungu');
        expect(branding.value.logoUrl).toBeNull();
        expect(branding.value.poweredBy).toBe(true);
    });

    it('set() merges partial updates', () => {
        branding.set({ brandName: 'Acme' });
        expect(branding.value.brandName).toBe('Acme');
        expect(branding.value.poweredBy).toBe(true);
        branding.set({ poweredBy: false, footerText: 'Acme Corp' });
        expect(branding.value.brandName).toBe('Acme');
        expect(branding.value.poweredBy).toBe(false);
        expect(branding.value.footerText).toBe('Acme Corp');
    });
});

describe('bootMeta', () => {
    beforeEach(() => {
        delete window.__RUNGU_META__;
    });

    it('returns empty object when the global is absent', () => {
        expect(bootMeta()).toEqual({});
    });

    it('reads a valid injected meta object', () => {
        window.__RUNGU_META__ = {
            brandName: 'Acme',
            logoUrl: 'https://acme.test/logo.png',
            footerText: 'Acme Corp',
            poweredBy: false,
        };
        expect(bootMeta()).toEqual({
            brandName: 'Acme',
            logoUrl: 'https://acme.test/logo.png',
            footerText: 'Acme Corp',
            poweredBy: false,
        });
    });

    it('rejects an empty or malformed brandName', () => {
        window.__RUNGU_META__ = { brandName: '', logoUrl: null, footerText: '', poweredBy: true };
        expect(bootMeta()).toEqual({});
        // @ts-expect-error — simulating a malformed injection
        window.__RUNGU_META__ = { brandName: 42 };
        expect(bootMeta()).toEqual({});
    });
});

describe('initFromWindow', () => {
    beforeEach(() => {
        branding.set({ ...DEFAULT_META });
        delete window.__RUNGU_META__;
    });

    it('applies the boot snapshot to the store', () => {
        window.__RUNGU_META__ = { brandName: 'Acme', logoUrl: null, footerText: '', poweredBy: false };
        initFromWindow();
        expect(branding.value.brandName).toBe('Acme');
        expect(branding.value.poweredBy).toBe(false);
    });

    it('is a no-op when nothing is injected', () => {
        initFromWindow();
        expect(branding.value).toEqual(DEFAULT_META);
    });
});
