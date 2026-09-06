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

/** Shape returned by `GET /api/projects/{slug}/roadmap`. */
export interface RoadmapResponse {
    planned: PostDetail[];
    planned_total: number;
    in_progress: PostDetail[];
    in_progress_total: number;
    done: PostDetail[];
    done_total: number;
    limit: number;
}

// ── Attachments ───────────────────────────────────────────────────────

export interface Attachment {
    id: string;
    post_id: string;
    filename: string;
    mime: string;
    size: number;
    url: string;
    created_by: string;
    created_at: string;
}

export interface AttachmentListResponse {
    data: Attachment[];
}

// ── Webhooks & admin ──────────────────────────────────────────────────

export interface Webhook {
    id: string;
    project_id: string;
    url: string;
    events: string;
    is_active: boolean;
    created_at: string;
}

export interface WebhookDelivery {
    id: string;
    webhook_id: string;
    event_type: string;
    status_code: number | null;
    success: boolean;
    attempts: number;
    last_error: string;
    created_at: string;
}

export interface WebhookTestResult {
    ok: boolean;
    status?: number;
    attempts?: number;
    error?: string;
}

export interface ProjectStats {
    total_posts: number;
    by_status: Record<string, number>;
    by_category: Record<string, number>;
    total_users: number;
    total_votes: number;
    total_comments: number;
}

// ── Analytics (#186/#187) ────────────────────────────────────────────────

export interface AnalyticsDailyRow {
    day: string;
    event_type: string;
    count: number;
}

export interface AnalyticsSummary {
    project_id: string;
    days: number;
    totals: Record<string, number>;
    daily: AnalyticsDailyRow[];
}

export interface AnalyticsTopRow {
    post_id: string;
    title: string;
    views: number;
    vote_count: number;
    vote_view_pct: number;
}
