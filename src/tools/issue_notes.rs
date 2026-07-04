use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List issue notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNotesListParams {
    pub project_id: crate::tools::ProjectId,
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
        "{}/issues/{}/notes",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/issues/{}/notes/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/issues/{}/notes",
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
// Update issue note
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteUpdateParams {
    pub project_id: crate::tools::ProjectId,
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
        "{}/issues/{}/notes/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/issues/{}/notes/{}",
        project_path(&p.project_id),
        p.issue_iid,
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

#[tool_router(router = tool_router_issue_notes, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List notes (comments) on a GitLab issue. Optional: order_by (\"created_at\" or \"updated_at\"), sort (\"asc\" or \"desc\"). Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_issues_notes_list(
        &self,
        Parameters(p): Parameters<IssueNotesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issue_notes_list, p, "issue notes")
    }

    #[tool(
        description = "Get a single note on a GitLab issue by note ID.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_issues_notes_get(
        &self,
        Parameters(p): Parameters<IssueNoteGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issue_note_get, p, "issue note")
    }

    #[tool(
        description = "Create a new note (comment) on a GitLab issue. Required: project_id, issue_iid, body. Optional: created_at (ISO 8601; requires administrator or Owner role).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_issues_notes_create(
        &self,
        Parameters(p): Parameters<IssueNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issue_note_create, p, "issue note")
    }

    #[tool(
        description = "Update the body of a note on a GitLab issue. Required: project_id, issue_iid, note_id, body.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_issues_notes_update(
        &self,
        Parameters(p): Parameters<IssueNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, issue_note_update, p, "issue note")
    }

    #[tool(
        description = "Delete a note from a GitLab issue. Required: project_id, issue_iid, note_id. This action is permanent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_issues_notes_delete(
        &self,
        Parameters(p): Parameters<IssueNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, issue_note_delete, p, "issue note")
    }
}
