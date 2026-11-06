import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import AuthProviderButtons from './AuthProviderButtons.svelte';
import { api } from '$lib/api/client';

vi.mock('$lib/api/client', async (importOriginal) => {
    const actual = await importOriginal<typeof import('$lib/api/client')>();
    return { ...actual, api: { ...actual.api, getProviders: vi.fn() } };
});

const mockApi = vi.mocked(api, true);

describe('AuthProviderButtons', () => {
    beforeEach(() => vi.clearAllMocks());

    it('labels the built-in providers itself and uses the label the server sends for OIDC', async () => {
        mockApi.getProviders.mockResolvedValue({
            providers: [
                { name: 'google', login_url: '/auth/google/login' },
                { name: 'keycloak', login_url: '/auth/keycloak/login' },
                { name: 'oidc', login_url: '/auth/oidc/login', label: 'CodeCora ID' },
            ],
        });
        render(AuthProviderButtons, { redirectTo: '/' });
        await waitFor(() => expect(screen.getByText('Continue with CodeCora ID')).toBeTruthy());
        expect(screen.getByText('Continue with Google')).toBeTruthy();
        expect(screen.getByText('Continue with Keycloak')).toBeTruthy();
    });

    it('falls back to the provider name when there is no label', async () => {
        mockApi.getProviders.mockResolvedValue({ providers: [{ name: 'oidc', login_url: '/auth/oidc/login' }] });
        render(AuthProviderButtons, { redirectTo: '/' });
        await waitFor(() => expect(screen.getByText('Continue with oidc')).toBeTruthy());
    });
});
