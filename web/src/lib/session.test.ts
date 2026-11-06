import { describe, it, expect, vi } from 'vitest';
import { ApiError } from '$lib/api/client';
import { createSession } from '$lib/session.svelte';
import type { CurrentUser } from '$lib/api/types';

const alice: CurrentUser = { id: 'u1', email: 'alice@example.test', name: 'Alice', role: 'member' } as CurrentUser;
const boss: CurrentUser = { ...alice, id: 'u2', email: 'boss@example.test', role: 'admin' } as CurrentUser;

describe('session', () => {
    it('starts loading and signs in on success', async () => {
        const s = createSession(async () => alice);
        expect(s.status).toBe('loading');
        await s.ready();
        expect(s.status).toBe('signed-in');
        expect(s.user).toEqual(alice);
        expect(s.signedIn).toBe(true);
        expect(s.isAdmin).toBe(false);
    });

    it('knows admins', async () => {
        const s = createSession(async () => boss);
        await s.ready();
        expect(s.isAdmin).toBe(true);
    });

    it('asks the server once however many pages wait for it', async () => {
        const fetchUser = vi.fn(async () => alice);
        const s = createSession(fetchUser);
        await Promise.all([s.ready(), s.ready(), s.ready()]);
        await s.ready(); // already settled
        expect(fetchUser).toHaveBeenCalledTimes(1);
    });

    it('a 401 means signed out, and stays settled', async () => {
        const fetchUser = vi.fn(async () => {
            throw new ApiError(401, 'unauthorized');
        });
        const s = createSession(fetchUser);
        await s.ready();
        await s.ready();
        expect(s.status).toBe('signed-out');
        expect(s.user).toBeNull();
        expect(fetchUser).toHaveBeenCalledTimes(1);
    });

    it('a 429, a server error or being offline is unknown, not signed out', async () => {
        for (const failure of [new ApiError(429, 'slow down'), new ApiError(503, 'down'), new TypeError('offline')]) {
            const s = createSession(async () => {
                throw failure;
            });
            await s.ready();
            expect(s.status).toBe('unknown');
            expect(s.user).toBeNull();
        }
    });

    it('a user we already knew stays signed in when a refresh cannot check', async () => {
        const fetchUser = vi
            .fn<() => Promise<CurrentUser>>()
            .mockResolvedValueOnce(alice)
            .mockRejectedValueOnce(new ApiError(429, 'slow down'));
        const s = createSession(fetchUser);
        await s.ready();
        await s.refresh();
        expect(s.status).toBe('unknown');
        expect(s.user).toEqual(alice);
        expect(s.signedIn).toBe(true);
    });

    it('a refresh that gets a 401 does sign the user out', async () => {
        const fetchUser = vi
            .fn<() => Promise<CurrentUser>>()
            .mockResolvedValueOnce(alice)
            .mockRejectedValueOnce(new ApiError(401, 'expired'));
        const s = createSession(fetchUser);
        await s.ready();
        await s.refresh();
        expect(s.status).toBe('signed-out');
        expect(s.user).toBeNull();
    });

    it('unknown retries on the next ready(), so a blip can recover', async () => {
        const fetchUser = vi
            .fn<() => Promise<CurrentUser>>()
            .mockRejectedValueOnce(new ApiError(429, 'slow down'))
            .mockResolvedValueOnce(alice);
        const s = createSession(fetchUser);
        await s.ready();
        expect(s.status).toBe('unknown');
        await s.ready();
        expect(s.status).toBe('signed-in');
        expect(fetchUser).toHaveBeenCalledTimes(2);
    });

    it('clear() signs out without asking the server', async () => {
        const fetchUser = vi.fn(async () => alice);
        const s = createSession(fetchUser);
        await s.ready();
        s.clear();
        expect(s.status).toBe('signed-out');
        expect(s.user).toBeNull();
        await s.ready();
        expect(fetchUser).toHaveBeenCalledTimes(1);
    });
});
