use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List MR discussions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionsListParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_discussions_list(client: &GitlabClient, p: MrDiscussionsListParams) -> ListResult {
    let path = format!(
        "{}/merge_requests/{}/discussions",
        project_path(&p.project_id),
        p.merge_request_iid
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// Get single MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionGetParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
}

pub async fn mr_discussion_get(
    client: &GitlabClient,
    p: MrDiscussionGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions/{}",
        project_path(&p.project_id),
        p.merge_request_iid,
        p.discussion_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionCreateParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Content of the discussion thread starter comment")]
    pub body: String,
    #[schemars(description = "SHA of the commit to pin this discussion to")]
    pub commit_id: Option<String>,
    // Diff note position fields — all optional; when any is set a `position` object is assembled.
    #[schemars(
        description = "Advanced: base commit SHA for a diff note position (requires position_head_sha and position_start_sha)"
    )]
    pub position_base_sha: Option<String>,
    #[schemars(description = "Advanced: HEAD commit SHA for a diff note position")]
    pub position_head_sha: Option<String>,
    #[schemars(description = "Advanced: start commit SHA for a diff note position")]
    pub position_start_sha: Option<String>,
    #[schemars(
        description = "Advanced: position type — \"text\", \"image\", or \"file\" (default: \"text\")"
    )]
    pub position_type: Option<String>,
    #[schemars(description = "Advanced: file path in the new (head) version for a diff note")]
    pub position_new_path: Option<String>,
    #[schemars(description = "Advanced: file path in the old (base) version for a diff note")]
    pub position_old_path: Option<String>,
    #[schemars(description = "Advanced: line number in the new version for a diff note")]
    pub position_new_line: Option<u64>,
    #[schemars(description = "Advanced: line number in the old version for a diff note")]
    pub position_old_line: Option<u64>,
}

fn build_position(p: &MrDiscussionCreateParams) -> Option<Value> {
    let has_position = p.position_base_sha.is_some()
        || p.position_head_sha.is_some()
        || p.position_start_sha.is_some()
        || p.position_new_path.is_some()
        || p.position_old_path.is_some()
        || p.position_new_line.is_some()
        || p.position_old_line.is_some()
        || p.position_type.is_some();

    if !has_position {
        return None;
    }

    let pos = BodyBuilder::new()
        .opt("base_sha", p.position_base_sha.as_deref())
        .opt("head_sha", p.position_head_sha.as_deref())
        .opt("start_sha", p.position_start_sha.as_deref())
        .req(
            "position_type",
            p.position_type.as_deref().unwrap_or("text"),
        )
        .opt("new_path", p.position_new_path.as_deref())
        .opt("old_path", p.position_old_path.as_deref())
        .opt("new_line", p.position_new_line)
        .opt("old_line", p.position_old_line)
        .build();
    Some(pos)
}

pub async fn mr_discussion_create(
    client: &GitlabClient,
    p: MrDiscussionCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions",
        project_path(&p.project_id),
        p.merge_request_iid
    );
    let position = build_position(&p);

    let body = BodyBuilder::new()
        .req("body", &p.body)
        .opt("commit_id", p.commit_id)
        .opt("position", position)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Resolve / unresolve MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionResolveParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "true to resolve the thread, false to unresolve it")]
    pub resolved: bool,
}

pub async fn mr_discussion_resolve(
    client: &GitlabClient,
    p: MrDiscussionResolveParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions/{}",
        project_path(&p.project_id),
        p.merge_request_iid,
        p.discussion_id
    );
    let body = BodyBuilder::new().req("resolved", p.resolved).build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Add note to MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionNoteCreateParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Content of the reply note")]
    pub body: String,
    #[schemars(
        description = "Set note creation time (ISO 8601); requires administrator or Owner role"
    )]
    pub created_at: Option<String>,
}

pub async fn mr_discussion_note_create(
    client: &GitlabClient,
    p: MrDiscussionNoteCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions/{}/notes",
        project_path(&p.project_id),
        p.merge_request_iid,
        p.discussion_id
    );
    let body = BodyBuilder::new()
        .req("body", &p.body)
        .opt("created_at", p.created_at)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update note in MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionNoteUpdateParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    pub note_id: crate::tools::NoteId,
    #[schemars(description = "New content for the note (mutually exclusive with resolved)")]
    pub body: Option<String>,
    #[schemars(
        description = "Resolve or unresolve the note (mutually exclusive with body; requires a resolvable thread)"
    )]
    pub resolved: Option<bool>,
}

pub async fn mr_discussion_note_update(
    client: &GitlabClient,
    p: MrDiscussionNoteUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions/{}/notes/{}",
        project_path(&p.project_id),
        p.merge_request_iid,
        p.discussion_id,
        p.note_id
    );
    let body = BodyBuilder::new()
        .opt("body", p.body)
        .opt("resolved", p.resolved)
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete note from MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionNoteDeleteParams {
    pub project_id: crate::tools::ProjectId,
    pub merge_request_iid: crate::tools::MergeRequestIid,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    pub note_id: crate::tools::NoteId,
}

pub async fn mr_discussion_note_delete(
    client: &GitlabClient,
    p: MrDiscussionNoteDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "{}/merge_requests/{}/discussions/{}/notes/{}",
        project_path(&p.project_id),
        p.merge_request_iid,
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

#[tool_router(router = tool_router_discussions, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List comments and discussion threads on a GitLab merge request (an MR's notes/comments live here). Each thread has an individual_note flag and a notes[] array; plain top-level comments appear as single-note threads. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_mrs_discussions_list(
        &self,
        Parameters(p): Parameters<MrDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, mr_discussions_list, p, "MR discussions")
    }

    #[tool(
        description = "Get a single comment thread (discussion) on a GitLab merge request by discussion ID (hex string).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_mrs_discussions_get(
        &self,
        Parameters(p): Parameters<MrDiscussionGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, mr_discussion_get, p, "MR discussion")
    }

    #[tool(
        description = "Comment on a GitLab merge request (creates a note / starts a discussion thread). To post a plain top-level comment, pass only body — this is the MR equivalent of gitlab_issues_notes_create. To pin an inline comment to a specific diff line, also pass the position_* fields. Required: project_id, merge_request_iid, body. Optional: commit_id (pin to commit SHA); inline diff-note position: position_base_sha, position_head_sha, position_start_sha, position_type (\"text\"/\"image\"/\"file\"), position_new_path, position_old_path, position_new_line, position_old_line.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_mrs_discussions_create(
        &self,
        Parameters(p): Parameters<MrDiscussionCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, mr_discussion_create, p, "MR discussion")
    }

    #[tool(
        description = "Resolve or unresolve a discussion thread on a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, resolved (true to resolve, false to unresolve). Requires Developer role or being the change author.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_mrs_discussions_resolve(
        &self,
        Parameters(p): Parameters<MrDiscussionResolveParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, mr_discussion_resolve, p, "MR discussion")
    }

    #[tool(
        description = "Reply to an existing comment thread (discussion) on a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, body. Optional: created_at (ISO 8601; requires administrator or Owner role).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_mrs_discussions_note_create(
        &self,
        Parameters(p): Parameters<MrDiscussionNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, mr_discussion_note_create, p, "MR discussion note")
    }

    #[tool(
        description = "Edit or resolve a comment (note) on a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, note_id. Provide exactly one of: body (new comment text) or resolved (true/false).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_mrs_discussions_note_update(
        &self,
        Parameters(p): Parameters<MrDiscussionNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, mr_discussion_note_update, p, "MR discussion note")
    }

    #[tool(
        description = "Delete a comment (note) from a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, note_id. This action is permanent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_mrs_discussions_note_delete(
        &self,
        Parameters(p): Parameters<MrDiscussionNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, mr_discussion_note_delete, p, "MR discussion note")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_create_params() -> MrDiscussionCreateParams {
        MrDiscussionCreateParams {
            project_id: "42".into(),
            merge_request_iid: 1.into(),
            body: "comment".into(),
            commit_id: None,
            position_base_sha: None,
            position_head_sha: None,
            position_start_sha: None,
            position_type: None,
            position_new_path: None,
            position_old_path: None,
            position_new_line: None,
            position_old_line: None,
        }
    }

    #[test]
    fn build_position_all_none_returns_none() {
        assert!(build_position(&base_create_params()).is_none());
    }

    #[test]
    fn build_position_defaults_type_to_text() {
        let p = MrDiscussionCreateParams {
            position_new_path: Some("src/main.rs".into()),
            position_new_line: Some(10),
            ..base_create_params()
        };
        let pos = build_position(&p).unwrap();
        assert_eq!(pos["position_type"], json!("text"));
    }

    #[test]
    fn build_position_respects_explicit_type() {
        let p = MrDiscussionCreateParams {
            position_type: Some("image".into()),
            position_new_path: Some("assets/logo.png".into()),
            ..base_create_params()
        };
        let pos = build_position(&p).unwrap();
        assert_eq!(pos["position_type"], json!("image"));
    }

    #[test]
    fn build_position_includes_only_provided_fields() {
        let p = MrDiscussionCreateParams {
            position_base_sha: Some("aaa".into()),
            position_head_sha: Some("bbb".into()),
            position_start_sha: Some("ccc".into()),
            position_new_path: Some("src/lib.rs".into()),
            position_new_line: Some(5),
            ..base_create_params()
        };
        let pos = build_position(&p).unwrap();
        assert_eq!(pos["base_sha"], json!("aaa"));
        assert_eq!(pos["head_sha"], json!("bbb"));
        assert_eq!(pos["start_sha"], json!("ccc"));
        assert_eq!(pos["new_path"], json!("src/lib.rs"));
        assert_eq!(pos["new_line"], json!(5));
        assert_eq!(pos["position_type"], json!("text"));
        assert!(pos.get("old_path").is_none());
        assert!(pos.get("old_line").is_none());
    }

    #[test]
    fn build_position_full_object() {
        let p = MrDiscussionCreateParams {
            position_base_sha: Some("aaa".into()),
            position_head_sha: Some("bbb".into()),
            position_start_sha: Some("ccc".into()),
            position_type: Some("text".into()),
            position_new_path: Some("src/lib.rs".into()),
            position_old_path: Some("src/old.rs".into()),
            position_new_line: Some(10),
            position_old_line: Some(8),
            ..base_create_params()
        };
        let pos = build_position(&p).unwrap();
        assert_eq!(pos["base_sha"], json!("aaa"));
        assert_eq!(pos["head_sha"], json!("bbb"));
        assert_eq!(pos["start_sha"], json!("ccc"));
        assert_eq!(pos["position_type"], json!("text"));
        assert_eq!(pos["new_path"], json!("src/lib.rs"));
        assert_eq!(pos["old_path"], json!("src/old.rs"));
        assert_eq!(pos["new_line"], json!(10));
        assert_eq!(pos["old_line"], json!(8));
    }
}
