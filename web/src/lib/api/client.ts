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
    RoadmapResponse,
    VoteResponse,
    ProviderInfo,
    Attachment,
    AttachmentListResponse,
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
        if (params?.page !== undefined) query.set('page', String(params.page));
        if (params?.per_page !== undefined) query.set('per_page', String(params.per_page));
        const qs = query.toString();
        return request<PaginatedResponse<PostDetail>>(
            `/api/projects/${slug}/posts${qs ? `?${qs}` : ''}`,
        );
    },

    /** Fetch the public roadmap (posts grouped by lifecycle status). Public endpoint. */
    getRoadmap: (slug: string, limit?: number) => {
        const qs = limit !== undefined ? `?limit=${limit}` : '';
        return request<DataResponse<RoadmapResponse>>(`/api/projects/${slug}/roadmap${qs}`).then((r) => r.data);
    },

    /** Fetch the changelog (done posts, newest ship first). Public, paginated. */
    getChangelog: (slug: string, params?: { page?: number; per_page?: number; since?: string }) => {
        const query = new URLSearchParams();
        if (params?.page !== undefined) query.set('page', String(params.page));
        if (params?.per_page !== undefined) query.set('per_page', String(params.per_page));
        if (params?.since) query.set('since', params.since);
        const qs = query.toString();
        return request<PaginatedResponse<PostDetail>>(`/api/projects/${slug}/changelog${qs ? `?${qs}` : ''}`);
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

    updatePostCategory: (id: string, category: PostCategory) =>
        request<DataResponse<PostDetail>>(`/api/posts/${id}`, {
            method: 'PATCH',
            body: JSON.stringify({ category }),
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

    // Attachments
    listAttachments: (postId: string) =>
        request<AttachmentListResponse>(`/api/posts/${postId}/attachments`).then((r) => r.data),

    uploadAttachment: async (postId: string, file: File) => {
        const formData = new FormData();
        formData.append('file', file);
        const res = await fetch(`/api/posts/${postId}/attachments`, {
            method: 'POST',
            body: formData,
            credentials: 'include',
        });
        if (!res.ok) {
            const err = await res.json().catch(() => ({ error: 'Upload failed' }));
            throw new ApiError(res.status, err.error || 'Upload failed');
        }
        return res.json().then((r: { data: Attachment }) => r.data);
    },

    deleteAttachment: (id: string) =>
        request<void>(`/api/attachments/${id}`, { method: 'DELETE' }),
};

export { ApiError };
