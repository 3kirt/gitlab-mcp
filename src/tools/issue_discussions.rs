use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id};

// --------------------------------------------------------------------------
// List issue discussions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionsListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_discussions_list(
    client: &GitlabClient,
    p: IssueDiscussionsListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get single issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
}

pub async fn issue_discussion_get(
    client: &GitlabClient,
    p: IssueDiscussionGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.discussion_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(
        description = "Content of the discussion thread starter comment (Markdown supported)"
    )]
    pub body: String,
    #[schemars(
        description = "Set discussion creation time (ISO 8601); requires administrator or Owner role"
    )]
    pub created_at: Option<String>,
}

pub async fn issue_discussion_create(
    client: &GitlabClient,
    p: IssueDiscussionCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let body = BodyBuilder::new()
        .req("body", &p.body)
        .opt("created_at", p.created_at)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Add note to issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionNoteCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Content of the reply note (Markdown supported)")]
    pub body: String,
    #[schemars(
        description = "Set note creation time (ISO 8601); requires administrator or Owner role"
    )]
    pub created_at: Option<String>,
}

pub async fn issue_discussion_note_create(
    client: &GitlabClient,
    p: IssueDiscussionNoteCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions/{}/notes",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.discussion_id
    );
    let body = BodyBuilder::new()
        .req("body", &p.body)
        .opt("created_at", p.created_at)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update note in issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionNoteUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
    #[schemars(description = "New content for the note (Markdown supported)")]
    pub body: String,
}

pub async fn issue_discussion_note_update(
    client: &GitlabClient,
    p: IssueDiscussionNoteUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.discussion_id,
        p.note_id
    );
    let body = BodyBuilder::new().req("body", &p.body).build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete note from issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionNoteDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
}

pub async fn issue_discussion_note_delete(
    client: &GitlabClient,
    p: IssueDiscussionNoteDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/discussions/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.discussion_id,
        p.note_id
    );
    client.delete(&path).await
}
