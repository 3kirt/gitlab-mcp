use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{PaginationParams, QueryBuilder};

// --------------------------------------------------------------------------
// List issues
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssuesListParams {
    #[schemars(description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")")]
    pub project_id: String,
    #[schemars(description = "Filter by state: \"opened\", \"closed\", or \"all\" (default: \"opened\")")]
    pub state: Option<String>,
    #[schemars(description = "Comma-separated label names to filter by")]
    pub labels: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(
        description = "Scope: \"created_by_me\", \"assigned_to_me\", or \"all\" (default: \"all\")"
    )]
    pub scope: Option<String>,
    #[schemars(description = "Filter by assignee user ID")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "Filter by author user ID")]
    pub author_id: Option<u64>,
    #[schemars(
        description = "Order by: \"created_at\", \"updated_at\", \"title\", \"priority\" (default: \"created_at\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issues_list(
    client: &GitlabClient,
    p: IssuesListParams,
) -> Result<Value, GitlabError> {
    let path = format!("/api/v4/projects/{}/issues", encode_project_id(&p.project_id));
    let params = QueryBuilder::new()
        .opt("state", p.state)
        .opt("labels", p.labels)
        .opt("search", p.search)
        .opt("scope", p.scope)
        .opt("assignee_id", p.assignee_id)
        .opt("author_id", p.author_id)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get single issue
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
}

pub async fn issue_get(client: &GitlabClient, p: IssueGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}",
        encode_project_id(&p.project_id),
        p.issue_iid
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create issue
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue title")]
    pub title: String,
    #[schemars(description = "Issue description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "Comma-separated label names")]
    pub labels: Option<String>,
    #[schemars(description = "User IDs to assign the issue to")]
    pub assignee_ids: Option<Vec<u64>>,
    #[schemars(description = "Milestone ID to associate with the issue")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Due date in YYYY-MM-DD format")]
    pub due_date: Option<String>,
    #[schemars(description = "Issue weight (GitLab EE only)")]
    pub weight: Option<u64>,
}

pub async fn issue_create(
    client: &GitlabClient,
    p: IssueCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!("/api/v4/projects/{}/issues", encode_project_id(&p.project_id));
    let mut body = json!({ "title": p.title });
    let obj = body.as_object_mut().unwrap();
    if let Some(v) = p.description {
        obj.insert("description".into(), json!(v));
    }
    if let Some(v) = p.labels {
        obj.insert("labels".into(), json!(v));
    }
    if let Some(v) = p.assignee_ids {
        obj.insert("assignee_ids".into(), json!(v));
    }
    if let Some(v) = p.milestone_id {
        obj.insert("milestone_id".into(), json!(v));
    }
    if let Some(v) = p.due_date {
        obj.insert("due_date".into(), json!(v));
    }
    if let Some(v) = p.weight {
        obj.insert("weight".into(), json!(v));
    }
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update issue
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "New issue title")]
    pub title: Option<String>,
    #[schemars(description = "New issue description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "State transition: \"close\" or \"reopen\"")]
    pub state_event: Option<String>,
    #[schemars(description = "Comma-separated label names (replaces existing labels)")]
    pub labels: Option<String>,
    #[schemars(description = "User IDs to assign the issue to (replaces existing assignees)")]
    pub assignee_ids: Option<Vec<u64>>,
    #[schemars(description = "Milestone ID (set to 0 to remove the milestone)")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Due date in YYYY-MM-DD format")]
    pub due_date: Option<String>,
    #[schemars(description = "Issue weight (GitLab EE only)")]
    pub weight: Option<u64>,
}

pub async fn issue_update(
    client: &GitlabClient,
    p: IssueUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}",
        encode_project_id(&p.project_id),
        p.issue_iid
    );
    let mut body = json!({});
    let obj = body.as_object_mut().unwrap();
    if let Some(v) = p.title {
        obj.insert("title".into(), json!(v));
    }
    if let Some(v) = p.description {
        obj.insert("description".into(), json!(v));
    }
    if let Some(v) = p.state_event {
        obj.insert("state_event".into(), json!(v));
    }
    if let Some(v) = p.labels {
        obj.insert("labels".into(), json!(v));
    }
    if let Some(v) = p.assignee_ids {
        obj.insert("assignee_ids".into(), json!(v));
    }
    if let Some(v) = p.milestone_id {
        obj.insert("milestone_id".into(), json!(v));
    }
    if let Some(v) = p.due_date {
        obj.insert("due_date".into(), json!(v));
    }
    if let Some(v) = p.weight {
        obj.insert("weight".into(), json!(v));
    }
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete issue
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
}

pub async fn issue_delete(client: &GitlabClient, p: IssueDeleteParams) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}",
        encode_project_id(&p.project_id),
        p.issue_iid
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// URL-encode a project ID for use in REST API paths.
/// Numeric IDs are passed through unchanged; path-style IDs like
/// "mygroup/myrepo" have slashes replaced with %2F.
fn encode_project_id(id: &str) -> String {
    if id.chars().all(|c| c.is_ascii_digit()) {
        id.to_string()
    } else {
        id.replace('/', "%2F")
    }
}
