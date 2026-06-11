use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// Shared CRUD helpers
//
// Every emoji-reaction endpoint follows the shape
//   {parent_path}/award_emoji[/{award_id}]
// where parent_path is the URL of the parent resource (issue, MR, snippet,
// or a note on one of those). The Params structs differ only in which
// parent-ID fields they carry; the bodies don't differ at all.
// --------------------------------------------------------------------------

async fn emoji_list(
    client: &GitlabClient,
    parent_path: &str,
    pagination: PaginationParams,
) -> ListResult {
    let path = format!("{parent_path}/award_emoji");
    list_paginated(client, &path, QueryBuilder::new(), pagination).await
}

async fn emoji_get(
    client: &GitlabClient,
    parent_path: &str,
    award_id: u64,
) -> Result<Value, GitlabError> {
    client
        .get(&format!("{parent_path}/award_emoji/{award_id}"))
        .await
}

async fn emoji_create(
    client: &GitlabClient,
    parent_path: &str,
    name: &str,
) -> Result<Value, GitlabError> {
    let body = BodyBuilder::new().req("name", name).build();
    client
        .post(&format!("{parent_path}/award_emoji"), &body)
        .await
}

async fn emoji_delete(
    client: &GitlabClient,
    parent_path: &str,
    award_id: u64,
) -> Result<(), GitlabError> {
    client
        .delete(&format!("{parent_path}/award_emoji/{award_id}"))
        .await
}

fn issue_path(project_id: &str, issue_iid: u64) -> String {
    format!("{}/issues/{}", project_path(project_id), issue_iid)
}

fn mr_path(project_id: &str, mr_iid: u64) -> String {
    format!("{}/merge_requests/{}", project_path(project_id), mr_iid)
}

fn snippet_path(project_id: &str, snippet_id: u64) -> String {
    format!("{}/snippets/{}", project_path(project_id), snippet_id)
}

/// Append `/notes/{note_id}` to a parent resource path, yielding the parent of
/// a note's award-emoji endpoints.
fn note_path(parent_path: &str, note_id: u64) -> String {
    format!("{parent_path}/notes/{note_id}")
}

// --------------------------------------------------------------------------
// Issues
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_emoji_list(client: &GitlabClient, p: IssueEmojiListParams) -> ListResult {
    emoji_list(
        client,
        &issue_path(&p.project_id, p.issue_iid),
        p.pagination,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_emoji_get(
    client: &GitlabClient,
    p: IssueEmojiGetParams,
) -> Result<Value, GitlabError> {
    emoji_get(client, &issue_path(&p.project_id, p.issue_iid), p.award_id).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn issue_emoji_create(
    client: &GitlabClient,
    p: IssueEmojiCreateParams,
) -> Result<Value, GitlabError> {
    emoji_create(client, &issue_path(&p.project_id, p.issue_iid), &p.name).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_emoji_delete(
    client: &GitlabClient,
    p: IssueEmojiDeleteParams,
) -> Result<(), GitlabError> {
    emoji_delete(client, &issue_path(&p.project_id, p.issue_iid), p.award_id).await
}

// --------------------------------------------------------------------------
// Merge Requests
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_emoji_list(client: &GitlabClient, p: MrEmojiListParams) -> ListResult {
    emoji_list(
        client,
        &mr_path(&p.project_id, p.merge_request_iid),
        p.pagination,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_emoji_get(
    client: &GitlabClient,
    p: MrEmojiGetParams,
) -> Result<Value, GitlabError> {
    emoji_get(
        client,
        &mr_path(&p.project_id, p.merge_request_iid),
        p.award_id,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn mr_emoji_create(
    client: &GitlabClient,
    p: MrEmojiCreateParams,
) -> Result<Value, GitlabError> {
    emoji_create(
        client,
        &mr_path(&p.project_id, p.merge_request_iid),
        &p.name,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_emoji_delete(
    client: &GitlabClient,
    p: MrEmojiDeleteParams,
) -> Result<(), GitlabError> {
    emoji_delete(
        client,
        &mr_path(&p.project_id, p.merge_request_iid),
        p.award_id,
    )
    .await
}

// --------------------------------------------------------------------------
// Snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippet_emoji_list(client: &GitlabClient, p: SnippetEmojiListParams) -> ListResult {
    emoji_list(
        client,
        &snippet_path(&p.project_id, p.snippet_id),
        p.pagination,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_emoji_get(
    client: &GitlabClient,
    p: SnippetEmojiGetParams,
) -> Result<Value, GitlabError> {
    emoji_get(
        client,
        &snippet_path(&p.project_id, p.snippet_id),
        p.award_id,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn snippet_emoji_create(
    client: &GitlabClient,
    p: SnippetEmojiCreateParams,
) -> Result<Value, GitlabError> {
    emoji_create(client, &snippet_path(&p.project_id, p.snippet_id), &p.name).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_emoji_delete(
    client: &GitlabClient,
    p: SnippetEmojiDeleteParams,
) -> Result<(), GitlabError> {
    emoji_delete(
        client,
        &snippet_path(&p.project_id, p.snippet_id),
        p.award_id,
    )
    .await
}

// --------------------------------------------------------------------------
// Issue Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_note_emoji_list(
    client: &GitlabClient,
    p: IssueNoteEmojiListParams,
) -> ListResult {
    let parent = note_path(&issue_path(&p.project_id, p.issue_iid), p.note_id);
    emoji_list(client, &parent, p.pagination).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_note_emoji_get(
    client: &GitlabClient,
    p: IssueNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&issue_path(&p.project_id, p.issue_iid), p.note_id);
    emoji_get(client, &parent, p.award_id).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn issue_note_emoji_create(
    client: &GitlabClient,
    p: IssueNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&issue_path(&p.project_id, p.issue_iid), p.note_id);
    emoji_create(client, &parent, &p.name).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_note_emoji_delete(
    client: &GitlabClient,
    p: IssueNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let parent = note_path(&issue_path(&p.project_id, p.issue_iid), p.note_id);
    emoji_delete(client, &parent, p.award_id).await
}

// --------------------------------------------------------------------------
// Merge Request Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_note_emoji_list(client: &GitlabClient, p: MrNoteEmojiListParams) -> ListResult {
    let parent = note_path(&mr_path(&p.project_id, p.merge_request_iid), p.note_id);
    emoji_list(client, &parent, p.pagination).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_note_emoji_get(
    client: &GitlabClient,
    p: MrNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&mr_path(&p.project_id, p.merge_request_iid), p.note_id);
    emoji_get(client, &parent, p.award_id).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn mr_note_emoji_create(
    client: &GitlabClient,
    p: MrNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&mr_path(&p.project_id, p.merge_request_iid), p.note_id);
    emoji_create(client, &parent, &p.name).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_note_emoji_delete(
    client: &GitlabClient,
    p: MrNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let parent = note_path(&mr_path(&p.project_id, p.merge_request_iid), p.note_id);
    emoji_delete(client, &parent, p.award_id).await
}

// --------------------------------------------------------------------------
// Snippet Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippet_note_emoji_list(
    client: &GitlabClient,
    p: SnippetNoteEmojiListParams,
) -> ListResult {
    let parent = note_path(&snippet_path(&p.project_id, p.snippet_id), p.note_id);
    emoji_list(client, &parent, p.pagination).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_note_emoji_get(
    client: &GitlabClient,
    p: SnippetNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&snippet_path(&p.project_id, p.snippet_id), p.note_id);
    emoji_get(client, &parent, p.award_id).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn snippet_note_emoji_create(
    client: &GitlabClient,
    p: SnippetNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let parent = note_path(&snippet_path(&p.project_id, p.snippet_id), p.note_id);
    emoji_create(client, &parent, &p.name).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_note_emoji_delete(
    client: &GitlabClient,
    p: SnippetNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let parent = note_path(&snippet_path(&p.project_id, p.snippet_id), p.note_id);
    emoji_delete(client, &parent, p.award_id).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_emoji_reactions, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List all emoji reactions on a GitLab issue. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_issues_list(
        &self,
        Parameters(p): Parameters<IssueEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issue_emoji_list, p, "issue emoji reactions")
    }

    #[tool(description = "Get a single emoji reaction on a GitLab issue by award ID.")]
    async fn gitlab_emoji_reactions_issues_get(
        &self,
        Parameters(p): Parameters<IssueEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issue_emoji_get, p, "issue emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab issue. Required: project_id, issue_iid, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_issues_create(
        &self,
        Parameters(p): Parameters<IssueEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issue_emoji_create, p, "issue emoji reaction")
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab issue. Only the reaction author or administrators may delete. Required: project_id, issue_iid, award_id."
    )]
    async fn gitlab_emoji_reactions_issues_delete(
        &self,
        Parameters(p): Parameters<IssueEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, issue_emoji_delete, p, "issue emoji reaction")
    }

    #[tool(
        description = "List all emoji reactions on a GitLab merge request. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_mrs_list(
        &self,
        Parameters(p): Parameters<MrEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, mr_emoji_list, p, "MR emoji reactions")
    }

    #[tool(description = "Get a single emoji reaction on a GitLab merge request by award ID.")]
    async fn gitlab_emoji_reactions_mrs_get(
        &self,
        Parameters(p): Parameters<MrEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, mr_emoji_get, p, "MR emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab merge request. Required: project_id, merge_request_iid, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_mrs_create(
        &self,
        Parameters(p): Parameters<MrEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, mr_emoji_create, p, "MR emoji reaction")
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab merge request. Only the reaction author or administrators may delete. Required: project_id, merge_request_iid, award_id."
    )]
    async fn gitlab_emoji_reactions_mrs_delete(
        &self,
        Parameters(p): Parameters<MrEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, mr_emoji_delete, p, "MR emoji reaction")
    }

    #[tool(
        description = "List all emoji reactions on a GitLab project snippet. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_snippets_list(
        &self,
        Parameters(p): Parameters<SnippetEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippet_emoji_list, p, "snippet emoji reactions")
    }

    #[tool(description = "Get a single emoji reaction on a GitLab project snippet by award ID.")]
    async fn gitlab_emoji_reactions_snippets_get(
        &self,
        Parameters(p): Parameters<SnippetEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippet_emoji_get, p, "snippet emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab project snippet. Required: project_id, snippet_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_snippets_create(
        &self,
        Parameters(p): Parameters<SnippetEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, snippet_emoji_create, p, "snippet emoji reaction")
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab project snippet. Only the reaction author or administrators may delete. Required: project_id, snippet_id, award_id."
    )]
    async fn gitlab_emoji_reactions_snippets_delete(
        &self,
        Parameters(p): Parameters<SnippetEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, snippet_emoji_delete, p, "snippet emoji reaction")
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab issue. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_issue_notes_list(
        &self,
        Parameters(p): Parameters<IssueNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issue_note_emoji_list, p, "issue note emoji reactions")
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab issue by award ID."
    )]
    async fn gitlab_emoji_reactions_issue_notes_get(
        &self,
        Parameters(p): Parameters<IssueNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issue_note_emoji_get, p, "issue note emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab issue. Required: project_id, issue_iid, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_issue_notes_create(
        &self,
        Parameters(p): Parameters<IssueNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            issue_note_emoji_create,
            p,
            "issue note emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab issue. Only the reaction author or administrators may delete. Required: project_id, issue_iid, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_issue_notes_delete(
        &self,
        Parameters(p): Parameters<IssueNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            issue_note_emoji_delete,
            p,
            "issue note emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab merge request. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_mr_notes_list(
        &self,
        Parameters(p): Parameters<MrNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, mr_note_emoji_list, p, "MR note emoji reactions")
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab merge request by award ID."
    )]
    async fn gitlab_emoji_reactions_mr_notes_get(
        &self,
        Parameters(p): Parameters<MrNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, mr_note_emoji_get, p, "MR note emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab merge request. Required: project_id, merge_request_iid, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_mr_notes_create(
        &self,
        Parameters(p): Parameters<MrNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, mr_note_emoji_create, p, "MR note emoji reaction")
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab merge request. Only the reaction author or administrators may delete. Required: project_id, merge_request_iid, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_mr_notes_delete(
        &self,
        Parameters(p): Parameters<MrNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, mr_note_emoji_delete, p, "MR note emoji reaction")
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab project snippet. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_list(
        &self,
        Parameters(p): Parameters<SnippetNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            snippet_note_emoji_list,
            p,
            "snippet note emoji reactions"
        )
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab project snippet by award ID."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_get(
        &self,
        Parameters(p): Parameters<SnippetNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            snippet_note_emoji_get,
            p,
            "snippet note emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab project snippet. Required: project_id, snippet_id, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_create(
        &self,
        Parameters(p): Parameters<SnippetNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            snippet_note_emoji_create,
            p,
            "snippet note emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab project snippet. Only the reaction author or administrators may delete. Required: project_id, snippet_id, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_delete(
        &self,
        Parameters(p): Parameters<SnippetNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            snippet_note_emoji_delete,
            p,
            "snippet note emoji reaction"
        )
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------
//
// These tests cover URL routing across the six emoji-reaction families. The
// real failure mode this guards against is path-template mix-ups (e.g. the
// issue family accidentally hitting the merge_requests URL after a refactor),
// so the assertions are deliberately path-shape focused. One representative
// from each family + the deepest nested family (issue notes) is enough; the
// other families share identical structure.

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        IssueEmojiCreateParams, IssueNoteEmojiCreateParams, MrEmojiDeleteParams,
        SnippetEmojiListParams, issue_emoji_create, issue_note_emoji_create, mr_emoji_delete,
        snippet_emoji_list,
    };
    use crate::test_util::mock_client;
    use crate::tools::PaginationParams;

    #[tokio::test]
    async fn issue_emoji_create_hits_issue_award_emoji_url_with_name_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/issues/7/award_emoji"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 1, "name": "thumbsup"
            })))
            .mount(&server)
            .await;

        let item = issue_emoji_create(
            &mock_client(&server),
            IssueEmojiCreateParams {
                project_id: "42".into(),
                issue_iid: 7,
                name: "thumbsup".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["name"], "thumbsup");

        let reqs = server.received_requests().await.unwrap();
        let body = reqs
            .iter()
            .find(|r| r.method == wiremock::http::Method::POST)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("POST request not found");
        assert_eq!(body["name"], "thumbsup");
    }

    #[tokio::test]
    async fn mr_emoji_delete_hits_merge_requests_award_emoji_url() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/42/merge_requests/3/award_emoji/99"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        mr_emoji_delete(
            &mock_client(&server),
            MrEmojiDeleteParams {
                project_id: "42".into(),
                merge_request_iid: 3,
                award_id: 99,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn snippet_emoji_list_hits_snippets_award_emoji_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/snippets/5/award_emoji"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        snippet_emoji_list(
            &mock_client(&server),
            SnippetEmojiListParams {
                project_id: "42".into(),
                snippet_id: 5,
                pagination: PaginationParams {
                    page: None,
                    per_page: None,
                    fetch_all: None,
                },
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn issue_note_emoji_create_hits_nested_notes_award_emoji_url() {
        // Deepest-nested family — the easiest to break by miswriting the path template.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/issues/7/notes/11/award_emoji"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 1, "name": "tada"
            })))
            .mount(&server)
            .await;

        issue_note_emoji_create(
            &mock_client(&server),
            IssueNoteEmojiCreateParams {
                project_id: "42".into(),
                issue_iid: 7,
                note_id: 11,
                name: "tada".into(),
            },
        )
        .await
        .unwrap();
    }
}
