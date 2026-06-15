import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api, ApiError } from '$lib/api/client';

// Mock global fetch
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

describe('API client', () => {
    beforeEach(() => {
        mockFetch.mockReset();
    });

    describe('ApiError', () => {
        it('stores status and message', () => {
            const err = new ApiError(404, 'Not found');
            expect(err.status).toBe(404);
            expect(err.message).toBe('Not found');
        });
    });

    describe('listProjects', () => {
        it('fetches projects from /api/projects', async () => {
            const mockProjects = {
                data: [
                    { id: '1', slug: 'test', name: 'Test', description: '', created_at: '2026-01-01' },
                ],
            };
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => mockProjects,
            });

            const result = await api.listProjects();
            expect(result).toHaveLength(1);
            expect(result[0].slug).toBe('test');
            expect(mockFetch).toHaveBeenCalledWith(
                '/api/projects',
                expect.objectContaining({ credentials: 'include' }),
            );
        });

        it('throws ApiError on failure', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: false,
                status: 500,
                json: async () => ({ error: 'Server error' }),
            });

            await expect(api.listProjects()).rejects.toThrow('Server error');
        });
    });

    describe('getPost', () => {
        it('fetches a single post', async () => {
            const mockPost = {
                data: {
                    id: 'post-1',
                    title: 'Test Post',
                    status: 'open',
                    category: 'feature',
                    vote_count: 5,
                },
            };
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => mockPost,
            });

            const result = await api.getPost('post-1');
            expect(result.title).toBe('Test Post');
            expect(result.vote_count).toBe(5);
        });
    });

    describe('createPost', () => {
        it('sends POST request with correct body', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 201,
                json: async () => ({ data: { id: 'new-1', title: 'New' } }),
            });

            await api.createPost('my-project', {
                title: 'New Post',
                description: 'Desc',
                category: 'feature',
            });

            expect(mockFetch).toHaveBeenCalledWith(
                '/api/projects/my-project/posts',
                expect.objectContaining({
                    method: 'POST',
                    body: JSON.stringify({ title: 'New Post', description: 'Desc', category: 'feature' }),
                }),
            );
        });
    });

    describe('toggleVote', () => {
        it('sends POST to vote endpoint', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => ({ data: { voted: true, vote_count: 42 } }),
            });

            const result = await api.toggleVote('post-1');
            expect(result.voted).toBe(true);
            expect(result.vote_count).toBe(42);
            expect(mockFetch).toHaveBeenCalledWith(
                '/api/posts/post-1/vote',
                expect.objectContaining({ method: 'POST' }),
            );
        });

        it('throws ApiError 401 when not authenticated', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: false,
                status: 401,
                json: async () => ({ error: 'Unauthorized' }),
            });

            await expect(api.toggleVote('post-1')).rejects.toThrow('Unauthorized');
        });
    });

    describe('listPosts', () => {
        it('builds query params correctly', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => ({ data: [], pagination: { page: 1, per_page: 20, total: 0, total_pages: 0 } }),
            });

            await api.listPosts('my-app', {
                sort: 'most_votes',
                status: 'open',
                category: 'bug',
                q: 'search term',
                page: 2,
                per_page: 10,
            });

            const calledUrl = mockFetch.mock.calls[0][0] as string;
            expect(calledUrl).toContain('sort=most_votes');
            expect(calledUrl).toContain('status=open');
            expect(calledUrl).toContain('category=bug');
            expect(calledUrl).toContain('q=search+term');
            expect(calledUrl).toContain('page=2');
            expect(calledUrl).toContain('per_page=10');
        });

        it('works without params', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => ({ data: [], pagination: { page: 1, per_page: 20, total: 0, total_pages: 0 } }),
            });

            await api.listPosts('my-app');
            const calledUrl = mockFetch.mock.calls[0][0] as string;
            expect(calledUrl).toBe('/api/projects/my-app/posts');
        });
    });

    describe('logout', () => {
        it('sends POST to /auth/logout', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: async () => ({ ok: true }),
            });

            await api.logout();
            expect(mockFetch).toHaveBeenCalledWith(
                '/auth/logout',
                expect.objectContaining({ method: 'POST' }),
            );
        });
    });
});
