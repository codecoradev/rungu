// API types — mirrors rungu-proto wire types (see crates/rungu-proto/src/lib.rs)

export type PostStatus = 'open' | 'planned' | 'in_progress' | 'done' | 'declined';
export type PostCategory = 'feedback' | 'bug' | 'feature' | 'question';
export type UserRole = 'admin' | 'member';

export interface User {
    id: string;
    email: string;
    name: string;
    avatar_url: string;
    role: UserRole;
}

export interface UserSummary {
    id: string;
    email: string;
    name: string;
    avatar_url: string;
}

export interface CurrentUser {
    id: string;
    email: string;
    role: UserRole;
}

export interface Project {
    id: string;
    slug: string;
    name: string;
    description: string;
    created_at: string;
}

export interface Post {
    id: string;
    project_id: string;
    title: string;
    description: string;
    status: PostStatus;
    category: PostCategory;
    vote_count: number;
    comment_count: number;
    created_by: string;
    created_at: string;
    updated_at: string;
}

export interface PostDetail {
    id: string;
    project_id: string;
    title: string;
    description: string;
    status: PostStatus;
    category: PostCategory;
    vote_count: number;
    comment_count: number;
    created_by: string;
    created_at: string;
    updated_at: string;
    creator: UserSummary;
    user_voted: boolean;
}

export interface Comment {
    id: string;
    post_id: string;
    parent_id: string | null;
    content: string;
    created_by: string;
    created_at: string;
    creator: UserSummary;
}

export interface VoteResponse {
    voted: boolean;
    vote_count: number;
}

export interface ProviderInfo {
    name: string;
    login_url: string;
}

// API response envelopes
export interface PaginatedResponse<T> {
    data: T[];
    pagination: {
        page: number;
        per_page: number;
        total: number;
        total_pages: number;
    };
}

export interface DataResponse<T> {
    data: T;
}

export interface ErrorResponse {
    error: string;
}
