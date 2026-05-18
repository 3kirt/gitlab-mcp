use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_project_id};

fn default_true() -> bool {
    true
}

// --------------------------------------------------------------------------
// List merge requests
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrsListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[schemars(
        description = "Filter by state: \"opened\", \"closed\", \"merged\", \"locked\", or \"all\" (default: \"all\")"
    )]
    pub state: Option<String>,
    #[schemars(description = "Filter by source branch name")]
    pub source_branch: Option<String>,
    #[schemars(description = "Filter by target branch name")]
    pub target_branch: Option<String>,
    #[schemars(description = "Filter by author user ID")]
    pub author_id: Option<u64>,
    #[schemars(description = "Filter by assignee user ID")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "Filter by reviewer user ID")]
    pub reviewer_id: Option<u64>,
    #[schemars(description = "Comma-separated label names to filter by")]
    pub labels: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(description = "Filter by draft status (true/false)")]
    pub draft: Option<bool>,
    #[schemars(
        description = "Scope: \"created_by_me\", \"assigned_to_me\", \"reviews_for_me\", or \"all\" (default: \"all\")"
    )]
    pub scope: Option<String>,
    #[schemars(
        description = "Order by: \"created_at\", \"updated_at\", \"merged_at\", \"title\" (default: \"created_at\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mrs_list(client: &GitlabClient, p: MrsListParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests",
        encode_project_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("state", p.state)
        .opt("source_branch", p.source_branch)
        .opt("target_branch", p.target_branch)
        .opt("author_id", p.author_id)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_id", p.reviewer_id)
        .opt("labels", p.labels)
        .opt("search", p.search)
        .opt("draft", p.draft)
        .opt("scope", p.scope)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Get single merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Merge request internal ID (IID) — the number shown in the GitLab UI"
    )]
    pub merge_request_iid: u64,
}

pub async fn mr_get(client: &GitlabClient, p: MrGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}",
        encode_project_id(&p.project_id),
        p.merge_request_iid
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Name of the source branch")]
    pub source_branch: String,
    #[schemars(description = "Name of the target branch")]
    pub target_branch: String,
    #[schemars(description = "Merge request title")]
    pub title: String,
    #[schemars(description = "Merge request description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "User ID to assign the MR to")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "User IDs to request reviews from")]
    pub reviewer_ids: Option<Vec<u64>>,
    #[schemars(description = "Comma-separated label names")]
    pub labels: Option<String>,
    #[schemars(description = "Milestone ID to associate with the MR")]
    pub milestone_id: Option<u64>,
    #[serde(default = "default_true")]
    #[schemars(description = "Squash commits on merge (default: true)")]
    pub squash: bool,
    #[serde(default = "default_true")]
    #[schemars(description = "Remove source branch after merge (default: true)")]
    pub remove_source_branch: bool,
    #[schemars(
        description = "Mark as draft (true/false); implemented via the \"Draft: \" title prefix"
    )]
    pub draft: Option<bool>,
}

pub async fn mr_create(client: &GitlabClient, p: MrCreateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests",
        encode_project_id(&p.project_id)
    );
    // GitLab ignores the `draft` body field; the title prefix is the reliable mechanism.
    let title = match p.draft {
        Some(true) if !p.title.starts_with("Draft:") => format!("Draft: {}", p.title),
        _ => p.title,
    };
    let body = BodyBuilder::new()
        .req("source_branch", &p.source_branch)
        .req("target_branch", &p.target_branch)
        .req("title", &title)
        .req("squash", p.squash)
        .req("remove_source_branch", p.remove_source_branch)
        .opt("description", p.description)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_ids", p.reviewer_ids)
        .opt("labels", p.labels)
        .opt("milestone_id", p.milestone_id)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "State transition: \"close\" or \"reopen\"")]
    pub state_event: Option<String>,
    #[schemars(description = "New target branch")]
    pub target_branch: Option<String>,
    #[schemars(description = "User ID to assign the MR to (0 to unassign)")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "User IDs to request reviews from (replaces existing reviewers)")]
    pub reviewer_ids: Option<Vec<u64>>,
    #[schemars(description = "Comma-separated label names (replaces existing labels)")]
    pub labels: Option<String>,
    #[schemars(description = "Milestone ID (0 to remove the milestone)")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Squash commits on merge (true/false)")]
    pub squash: Option<bool>,
    #[schemars(
        description = "Mark as draft (true) or ready to merge (false); adds or removes the \"Draft: \" title prefix"
    )]
    pub draft: Option<bool>,
}

pub async fn mr_update(client: &GitlabClient, p: MrUpdateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}",
        encode_project_id(&p.project_id),
        p.merge_request_iid
    );
    // GitLab's update endpoint doesn't accept `draft` as a body field; draft status is
    // controlled by the "Draft: " title prefix. Fetch the current title when needed.
    let effective_title = match p.draft {
        Some(make_draft) => {
            let base = match p.title {
                Some(ref t) => t.clone(),
                None => {
                    let current = client.get(&path).await?;
                    current["title"].as_str().unwrap_or("").to_string()
                }
            };
            Some(if make_draft {
                if base.starts_with("Draft:") {
                    base
                } else {
                    format!("Draft: {}", base)
                }
            } else {
                base.strip_prefix("Draft: ")
                    .or_else(|| base.strip_prefix("Draft:"))
                    .unwrap_or(&base)
                    .to_string()
            })
        }
        None => p.title,
    };
    let body = BodyBuilder::new()
        .opt("title", effective_title)
        .opt("description", p.description)
        .opt("state_event", p.state_event)
        .opt("target_branch", p.target_branch)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_ids", p.reviewer_ids)
        .opt("labels", p.labels)
        .opt("milestone_id", p.milestone_id)
        .opt("squash", p.squash)
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
}

pub async fn mr_delete(client: &GitlabClient, p: MrDeleteParams) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}",
        encode_project_id(&p.project_id),
        p.merge_request_iid
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Merge (accept) merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrMergeParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "Custom merge commit message")]
    pub merge_commit_message: Option<String>,
    #[schemars(description = "Squash commits on merge (true/false)")]
    pub squash: Option<bool>,
    #[schemars(description = "Remove source branch after merge (true/false)")]
    pub should_remove_source_branch: Option<bool>,
    #[schemars(description = "Merge only once the pipeline succeeds (true/false)")]
    pub merge_when_pipeline_succeeds: Option<bool>,
}

pub async fn mr_merge(client: &GitlabClient, p: MrMergeParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/merge",
        encode_project_id(&p.project_id),
        p.merge_request_iid
    );
    let body = BodyBuilder::new()
        .opt("merge_commit_message", p.merge_commit_message)
        .opt("squash", p.squash)
        .opt("should_remove_source_branch", p.should_remove_source_branch)
        .opt(
            "merge_when_pipeline_succeeds",
            p.merge_when_pipeline_succeeds,
        )
        .build();
    client.put(&path, &body).await
}
