// API client — typed fetch wrapper for the Rungu REST API.
// All endpoints mirror the OpenAPI spec (see /swagger-ui when server is running).

import type {
    CurrentUser,
    DataResponse,
    PaginatedResponse,
    Post,
    PostDetail,
    PostCategory,
    PostStatus,
    Project,
    Comment,
    VoteResponse,
    ProviderInfo,
} from './types';

const BASE = '';

class ApiError extends Error {
    constructor(
        public status: number,
        message: string,
    ) {
        super(message);
    }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
    const res = await fetch(`${BASE}${path}`, {
        credentials: 'include',
        headers: {
            'Content-Type': 'application/json',
            ...options?.headers,
        },
        ...options,
    });

    if (!res.ok) {
        let message = `HTTP ${res.status}`;
        try {
            const body = await res.json();
            message = body.error || message;
        } catch {
            // ignore parse error
        }
        throw new ApiError(res.status, message);
    }

    if (res.status === 204) {
        return undefined as T;
    }

    return res.json();
}

// ── Auth ────────────────────────────────────────────────────────────

export const api = {
    // Auth
    getProviders: () => request<{ providers: ProviderInfo[] }>('/auth/providers'),

    getCurrentUser: () => request<DataResponse<CurrentUser>>('/auth/me').then((r) => r.data),

    login: (provider: string) => {
        window.location.href = `${BASE}/auth/${provider}/login`;
    },

    logout: () =>
        request<{ ok: boolean }>('/auth/logout', { method: 'POST' }),

    // Projects
    listProjects: () => request<{ data: Project[] }>('/api/projects').then((r) => r.data),

    getProject: (slug: string) =>
        request<DataResponse<Project>>(`/api/projects/${slug}`).then((r) => r.data),

    createProject: (body: { name: string; slug?: string; description?: string }) =>
        request<DataResponse<Project>>('/api/projects', {
            method: 'POST',
            body: JSON.stringify(body),
        }).then((r) => r.data),

    updateProject: (slug: string, body: { name?: string; description?: string }) =>
        request<DataResponse<Project>>(`/api/projects/${slug}`, {
            method: 'PATCH',
            body: JSON.stringify(body),
        }).then((r) => r.data),

    deleteProject: (slug: string) =>
        request<void>(`/api/projects/${slug}`, { method: 'DELETE' }),

    // Posts
    listPosts: (slug: string, params?: {
        sort?: string;
        status?: PostStatus;
        category?: PostCategory;
        q?: string;
        page?: number;
        per_page?: number;
    }) => {
        const query = new URLSearchParams();
        if (params?.sort) query.set('sort', params.sort);
        if (params?.status) query.set('status', params.status);
        if (params?.category) query.set('category', params.category);
        if (params?.q) query.set('q', params.q);
        if (params?.page) query.set('page', String(params.page));
        if (params?.per_page) query.set('per_page', String(params.per_page));
        const qs = query.toString();
        return request<PaginatedResponse<PostDetail>>(
            `/api/projects/${slug}/posts${qs ? `?${qs}` : ''}`,
        );
    },

    getPost: (id: string) =>
        request<DataResponse<PostDetail>>(`/api/posts/${id}`).then((r) => r.data),

    createPost: (slug: string, body: { title: string; description?: string; category?: PostCategory }) =>
        request<DataResponse<Post>>(`/api/projects/${slug}/posts`, {
            method: 'POST',
            body: JSON.stringify(body),
        }).then((r) => r.data),

    updatePostStatus: (id: string, status: PostStatus) =>
        request<DataResponse<PostDetail>>(`/api/posts/${id}`, {
            method: 'PATCH',
            body: JSON.stringify({ status }),
        }).then((r) => r.data),

    deletePost: (id: string) => request<void>(`/api/posts/${id}`, { method: 'DELETE' }),

    // Votes
    toggleVote: (id: string) =>
        request<DataResponse<VoteResponse>>(`/api/posts/${id}/vote`, {
            method: 'POST',
        }).then((r) => r.data),

    checkVoted: (id: string) =>
        request<DataResponse<{ voted: boolean }>>(`/api/posts/${id}/vote`).then((r) => r.data),

    // Comments
    listComments: (postId: string) =>
        request<{ data: Comment[] }>(`/api/posts/${postId}/comments`).then((r) => r.data),

    createComment: (postId: string, body: { content: string; parent_id?: string }) =>
        request<DataResponse<Comment>>(`/api/posts/${postId}/comments`, {
            method: 'POST',
            body: JSON.stringify(body),
        }).then((r) => r.data),

    deleteComment: (id: string) =>
        request<void>(`/api/comments/${id}`, { method: 'DELETE' }),
};

export { ApiError };
