use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List issue discussions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionsListParams {
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_discussions_list(
    client: &GitlabClient,
    p: IssueDiscussionsListParams,
) -> ListResult {
    let path = format!(
        "{}/issues/{}/discussions",
        project_path(&p.project_id),
        p.issue_iid
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// Get single issue discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDiscussionGetParams {
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
}

pub async fn issue_discussion_get(
    client: &GitlabClient,
    p: IssueDiscussionGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/issues/{}/discussions/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
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
        "{}/issues/{}/discussions",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
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
        "{}/issues/{}/discussions/{}/notes",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    pub note_id: crate::tools::NoteId,
    #[schemars(description = "New content for the note (Markdown supported)")]
    pub body: String,
}

pub async fn issue_discussion_note_update(
    client: &GitlabClient,
    p: IssueDiscussionNoteUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/issues/{}/discussions/{}/notes/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
    pub issue_iid: crate::tools::IssueIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    pub note_id: crate::tools::NoteId,
}

pub async fn issue_discussion_note_delete(
    client: &GitlabClient,
    p: IssueDiscussionNoteDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "{}/issues/{}/discussions/{}/notes/{}",
        project_path(&p.project_id),
        p.issue_iid,
        p.discussion_id,
        p.note_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_issue_discussions, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List comments and discussion threads on a GitLab issue (thread-grouped view). Each thread has an individual_note flag and a notes[] array. For a flat list of the same comments, use gitlab_issues_notes_list. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_issues_discussions_list(
        &self,
        Parameters(p): Parameters<IssueDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issue_discussions_list, p, "issue discussions")
    }

    #[tool(
        description = "Get a single comment thread (discussion) on a GitLab issue by discussion ID (hex string).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_issues_discussions_get(
        &self,
        Parameters(p): Parameters<IssueDiscussionGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issue_discussion_get, p, "issue discussion")
    }

    #[tool(
        description = "Start a comment thread (discussion) on a GitLab issue. To post a plain comment, gitlab_issues_notes_create is the simpler equivalent. Required: project_id, issue_iid, body. Optional: created_at (ISO 8601; requires administrator or Owner role).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_issues_discussions_create(
        &self,
        Parameters(p): Parameters<IssueDiscussionCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issue_discussion_create, p, "issue discussion")
    }

    #[tool(
        description = "Reply to an existing comment thread (discussion) on a GitLab issue. Required: project_id, issue_iid, discussion_id, body. Optional: created_at (ISO 8601; requires administrator or Owner role).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_issues_discussions_note_create(
        &self,
        Parameters(p): Parameters<IssueDiscussionNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            issue_discussion_note_create,
            p,
            "issue discussion note"
        )
    }

    #[tool(
        description = "Edit the body of a comment (note) in a GitLab issue discussion thread. Required: project_id, issue_iid, discussion_id, note_id, body.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_issues_discussions_note_update(
        &self,
        Parameters(p): Parameters<IssueDiscussionNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            issue_discussion_note_update,
            p,
            "issue discussion note"
        )
    }

    #[tool(
        description = "Delete a comment (note) from a GitLab issue discussion thread. Required: project_id, issue_iid, discussion_id, note_id. This action is permanent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_issues_discussions_note_delete(
        &self,
        Parameters(p): Parameters<IssueDiscussionNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            issue_discussion_note_delete,
            p,
            "issue discussion note"
        )
    }
}
