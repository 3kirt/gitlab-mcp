use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, list_paginated,
};

// --------------------------------------------------------------------------
// List MR discussions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionsListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_discussions_list(client: &GitlabClient, p: MrDiscussionsListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/discussions",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// Get single MR discussion
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDiscussionGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
}

pub async fn mr_discussion_get(
    client: &GitlabClient,
    p: MrDiscussionGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/discussions/{}",
        encode_namespace_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
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
        "/api/v4/projects/{}/merge_requests/{}/discussions",
        encode_namespace_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
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
        "/api/v4/projects/{}/merge_requests/{}/discussions/{}",
        encode_namespace_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
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
        "/api/v4/projects/{}/merge_requests/{}/discussions/{}/notes",
        encode_namespace_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
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
        "/api/v4/projects/{}/merge_requests/{}/discussions/{}/notes/{}",
        encode_namespace_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "Discussion ID (hex string)")]
    pub discussion_id: String,
    #[schemars(description = "Note ID (integer)")]
    pub note_id: u64,
}

pub async fn mr_discussion_note_delete(
    client: &GitlabClient,
    p: MrDiscussionNoteDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/discussions/{}/notes/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.discussion_id,
        p.note_id
    );
    client.delete(&path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_create_params() -> MrDiscussionCreateParams {
        MrDiscussionCreateParams {
            project_id: "42".into(),
            merge_request_iid: 1,
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
