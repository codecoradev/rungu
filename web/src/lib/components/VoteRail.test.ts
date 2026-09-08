import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import VoteRail from './VoteRail.svelte';
import { api, ApiError } from '$lib/api/client';
import { toastError } from '$lib/toast.svelte';

vi.mock('$lib/api/client', async (importOriginal) => {
    const actual = await importOriginal<typeof import('$lib/api/client')>();
    return {
        ...actual,
        api: { ...actual.api, toggleVote: vi.fn() },
        ApiError: actual.ApiError,
    };
});

vi.mock('$lib/toast.svelte', () => ({
    toastError: vi.fn(),
}));

const mockApi = vi.mocked(api, true);

describe('VoteRail', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders count and exposes an accessible vote control', () => {
        render(VoteRail, { postId: 'p1', voted: false, count: 7 });
        expect(screen.getByRole('button', { name: 'Vote' })).toBeTruthy();
        expect(screen.getByText('7')).toBeTruthy();
        expect(screen.getByRole('button').getAttribute('aria-pressed')).toBe('false');
    });

    it('shows the voted state (aria-pressed + label swap)', () => {
        render(VoteRail, { postId: 'p1', voted: true, count: 8 });
        const btn = screen.getByRole('button', { name: 'Remove vote' });
        expect(btn.getAttribute('aria-pressed')).toBe('true');
    });

    it('optimistically increments, reconciles with the server, and notifies onvote', async () => {
        mockApi.toggleVote.mockResolvedValueOnce({ voted: true, vote_count: 8 });
        const onvote = vi.fn();
        render(VoteRail, { postId: 'p1', voted: false, count: 7, onvote });

        fireEvent.click(screen.getByRole('button', { name: 'Vote' }));
        // Optimistic state visible immediately
        expect(screen.getByText('8')).toBeTruthy();

        await waitFor(() => expect(mockApi.toggleVote).toHaveBeenCalledWith('p1'));
        await waitFor(() => expect(onvote).toHaveBeenLastCalledWith(true, 8));
        await waitFor(() => expect(screen.getByRole('button', { name: 'Remove vote' })).toBeTruthy());
    });

    it('reverts the optimistic update on failure and raises a toast', async () => {
        mockApi.toggleVote.mockRejectedValueOnce(new Error('network down'));
        const onvote = vi.fn();
        render(VoteRail, { postId: 'p1', voted: false, count: 7, onvote });

        fireEvent.click(screen.getByRole('button', { name: 'Vote' }));
        expect(screen.getByText('8')).toBeTruthy(); // optimistic

        await waitFor(() => expect(screen.getByText('7')).toBeTruthy()); // reverted
        await waitFor(() => expect(onvote).toHaveBeenLastCalledWith(false, 7));
        await waitFor(() => expect(toastError).toHaveBeenCalledWith('Failed to vote', { action: undefined }));
        expect(screen.getByRole('button', { name: 'Vote' })).toBeTruthy();
    });

    it('offers a login action on 401', async () => {
        mockApi.toggleVote.mockRejectedValueOnce(new ApiError(401, 'unauthorized'));
        render(VoteRail, { postId: 'p1', voted: false, count: 2 });

        fireEvent.click(screen.getByRole('button', { name: 'Vote' }));
        await waitFor(() =>
            expect(toastError).toHaveBeenCalledWith(
                'Login to vote',
                expect.objectContaining({ action: expect.objectContaining({ href: expect.stringContaining('/login?redirect=') }) }),
            ),
        );
    });

    it('does not fire requests while disabled', async () => {
        render(VoteRail, { postId: 'p1', voted: false, count: 2, disabled: true });
        fireEvent.click(screen.getByRole('button', { name: 'Vote' }));
        expect(mockApi.toggleVote).not.toHaveBeenCalled();
    });
});
