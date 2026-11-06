import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import PostForm from './PostForm.svelte';
import { api } from '$lib/api/client';

vi.mock('$lib/api/client', async (importOriginal) => {
    const actual = await importOriginal<typeof import('$lib/api/client')>();
    return { ...actual, api: { ...actual.api, getSimilarByTitle: vi.fn() } };
});

const mockApi = vi.mocked(api, true);

const existing = {
    id: 'p1',
    project_id: 'proj',
    title: 'Dark mode for the widget',
    description: '',
    category: 'feature',
    status: 'open',
    vote_count: 7,
    comment_count: 0,
    created_by: 'u1',
    created_at: '2026-11-01T00:00:00Z',
    updated_at: '2026-11-01T00:00:00Z',
    creator: { id: 'u1', name: 'Rina', avatar_url: '' },
    user_voted: false,
};

describe('PostForm duplicate check', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
    });
    afterEach(() => {
        vi.useRealTimers();
    });

    async function typeTitle(value: string) {
        await fireEvent.input(screen.getByLabelText('Title'), { target: { value } });
        await vi.advanceTimersByTimeAsync(450);
    }

    it('suggests existing posts that match the draft title, linking to them', async () => {
        mockApi.getSimilarByTitle.mockResolvedValue([existing] as never);
        render(PostForm, { slug: 'demo', onsubmit: vi.fn(), showTitle: false });

        await typeTitle('Please add dark mode');

        expect(mockApi.getSimilarByTitle).toHaveBeenCalledWith('demo', 'Please add dark mode');
        expect(screen.getByText(/Similar posts already exist/)).toBeTruthy();
        const link = screen.getByRole('link', { name: /Dark mode for the widget/ });
        expect(link.getAttribute('href')).toBe('/board/demo/post/p1');
    });

    it('waits for a word of 4+ characters before asking the server', async () => {
        render(PostForm, { slug: 'demo', onsubmit: vi.fn(), showTitle: false });
        await typeTitle('a b c');
        expect(mockApi.getSimilarByTitle).not.toHaveBeenCalled();
        expect(screen.queryByText(/Similar posts already exist/)).toBeNull();
    });

    it('shows nothing when the lookup fails', async () => {
        mockApi.getSimilarByTitle.mockRejectedValue(new Error('offline'));
        render(PostForm, { slug: 'demo', onsubmit: vi.fn(), showTitle: false });
        await typeTitle('Please add dark mode');
        expect(screen.queryByText(/Similar posts already exist/)).toBeNull();
    });
});
