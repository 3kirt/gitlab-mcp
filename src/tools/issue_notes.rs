use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, list_paginated,
};

// --------------------------------------------------------------------------
// List issue notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNotesListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(
        description = "Order by: \"created_at\" or \"updated_at\" (default: \"created_at\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_notes_list(client: &GitlabClient, p: IssueNotesListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let qb = QueryBuilder::new()
        .opt("order_by", p.order_by)
        .opt("sort", p.sort);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get single issue note
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
}

pub async fn issue_note_get(
    client: &GitlabClient,
    p: IssueNoteGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create issue note
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Content of the note (Markdown supported)")]
    pub body: String,
    #[schemars(
        description = "Set note creation time (ISO 8601); requires administrator or Owner role"
    )]
    pub created_at: Option<String>,
}

pub async fn issue_note_create(
    client: &GitlabClient,
    p: IssueNoteCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes",
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
// Update issue note
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
    #[schemars(description = "New content for the note (Markdown supported)")]
    pub body: String,
}

pub async fn issue_note_update(
    client: &GitlabClient,
    p: IssueNoteUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id
    );
    let body = BodyBuilder::new().req("body", &p.body).build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete issue note
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
}

pub async fn issue_note_delete(
    client: &GitlabClient,
    p: IssueNoteDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id
    );
    client.delete(&path).await
}
