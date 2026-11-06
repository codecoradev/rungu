// Session — who is signed in, loaded once and read by every page.
//
// Before this module the layout, the board, the post page, the admin page and
// the login page each called `GET /auth/me` on their own, and treated every
// failure as "signed out". Now the layout loads the session once and pages
// read it; `ready()` shares the one in-flight request.
//
// Three outcomes are kept apart:
//   - signed in:   we know the user
//   - signed out:  the server said 401
//   - unknown:     the check itself failed (429, 5xx, network) — we could not
//                  tell. A user we already knew stays shown as signed in.

import { api, ApiError } from '$lib/api/client';
import type { CurrentUser } from '$lib/api/types';

export type SessionStatus = 'loading' | 'signed-in' | 'signed-out' | 'unknown';

export interface Session {
    readonly user: CurrentUser | null;
    readonly status: SessionStatus;
    readonly isAdmin: boolean;
    /** True when the server confirmed (or we already knew) a signed-in user. */
    readonly signedIn: boolean;
    /** Settle the session if it isn't yet, sharing any request in flight.
     *  Settled states (signed in / out) are answered without a request;
     *  `unknown` retries, so a transient failure can recover on navigation. */
    ready(): Promise<void>;
    /** Ask the server again (e.g. a "retry" button). */
    refresh(): Promise<void>;
    /** Forget the user (after logout). */
    clear(): void;
}

/** Build a session around a way to fetch the current user. `api` is the seam:
 *  production passes the fetch client, tests pass a fake. */
export function createSession(fetchUser: () => Promise<CurrentUser>): Session {
    let user = $state<CurrentUser | null>(null);
    let status = $state<SessionStatus>('loading');
    let inflight: Promise<void> | null = null;

    async function load(): Promise<void> {
        try {
            user = await fetchUser();
            status = 'signed-in';
        } catch (e) {
            if (e instanceof ApiError && e.status === 401) {
                user = null;
                status = 'signed-out';
            } else {
                // Rate limited, server error or offline: we couldn't check.
                // Keep whoever we knew about rather than signing them out.
                status = 'unknown';
            }
        }
    }

    function start(): Promise<void> {
        inflight ??= load().finally(() => {
            inflight = null;
        });
        return inflight;
    }

    return {
        get user() {
            return user;
        },
        get status() {
            return status;
        },
        get isAdmin() {
            return user?.role === 'admin';
        },
        get signedIn() {
            return user !== null;
        },
        ready() {
            if (inflight) return inflight;
            return status === 'signed-in' || status === 'signed-out' ? Promise.resolve() : start();
        },
        refresh: start,
        clear() {
            user = null;
            status = 'signed-out';
        },
    };
}

/** The app's session, backed by `GET /auth/me`. */
export const session = createSession(() => api.getCurrentUser());
