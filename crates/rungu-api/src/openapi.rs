//! OpenAPI documentation for the Rungu API.
//!
//! Served via Swagger UI at `/swagger-ui` and the raw spec at `/api-docs/openapi.json`.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Rungu API",
        version = "0.1.0",
        description = "Lightweight self-hosted feedback board API.\n\nCollect feature requests, bug reports, and suggestions with voting and commenting.",
        license(name = "Apache-2.0", url = "https://www.apache.org/licenses/LICENSE-2.0"),
    ),
    paths(
        // Projects
        crate::project_routes::list_projects,
        crate::project_routes::create_project,
        crate::project_routes::get_project,
        crate::project_routes::update_project,
        crate::project_routes::delete_project,
        // Posts
        crate::post_routes::list_posts,
        crate::post_routes::create_post,
        crate::post_routes::get_post,
        crate::post_routes::update_post,
        crate::post_routes::delete_post,
        crate::post_routes::get_project_roadmap,
        crate::post_routes::get_project_changelog,
        // Votes
        crate::vote_routes::toggle_vote,
        crate::vote_routes::check_voted,
        // Comments
        crate::comment_routes::list_comments,
        crate::comment_routes::create_comment,
        crate::comment_routes::delete_comment,
        // Attachments
        crate::attachment_routes::list_attachments,
        crate::attachment_routes::upload_attachment,
        crate::attachment_routes::get_attachment_file,
        crate::attachment_routes::delete_attachment,
    ),
    components(schemas(
        rungu_proto::Post,
        rungu_proto::PostDetail,
        rungu_proto::Project,
        rungu_proto::Comment,
        rungu_proto::CommentDetail,
        rungu_proto::PostStatus,
        rungu_proto::PostCategory,
        rungu_proto::UserSummary,
        rungu_proto::UserRole,
        rungu_proto::ProviderInfo,
        rungu_proto::CreatePostBody,
        rungu_proto::UpdatePostBody,
        rungu_proto::CreateCommentBody,
        rungu_proto::CreateProjectBody,
        rungu_proto::UpdateProjectBody,
        rungu_proto::VoteToggleResponse,
        rungu_proto::VoteStatusResponse,
        rungu_proto::Attachment,
        rungu_proto::AttachmentResponse,
    )),
    tags(
        (name = "projects", description = "Project management endpoints"),
        (name = "posts", description = "Feedback post endpoints"),
        (name = "votes", description = "Voting endpoints"),
        (name = "comments", description = "Comment endpoints"),
        (name = "attachments", description = "Image attachment endpoints"),
    ),
)]
pub struct ApiDoc;
