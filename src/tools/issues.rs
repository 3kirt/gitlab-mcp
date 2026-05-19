use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_project_id};

// --------------------------------------------------------------------------
// List issues
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssuesListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[schemars(
        description = "Filter by state: \"opened\", \"closed\", or \"all\" (default: \"all\" — GitLab returns all issues when omitted)"
    )]
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
    #[schemars(description = "Return only issues created after this datetime (ISO 8601, e.g. \"2024-01-01T00:00:00Z\")")]
    pub created_after: Option<String>,
    #[schemars(description = "Return only issues created before this datetime (ISO 8601)")]
    pub created_before: Option<String>,
    #[schemars(description = "Return only issues updated after this datetime (ISO 8601)")]
    pub updated_after: Option<String>,
    #[schemars(description = "Return only issues updated before this datetime (ISO 8601)")]
    pub updated_before: Option<String>,
    #[schemars(
        description = "Order by: \"created_at\", \"updated_at\", \"title\", \"priority\" (default: \"created_at\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issues_list(client: &GitlabClient, p: IssuesListParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues",
        encode_project_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("state", p.state)
        .opt("labels", p.labels)
        .opt("search", p.search)
        .opt("scope", p.scope)
        .opt("assignee_id", p.assignee_id)
        .opt("author_id", p.author_id)
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("updated_after", p.updated_after)
        .opt("updated_before", p.updated_before)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.get_with_params(&path, &params).await
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
    let path = format!(
        "/api/v4/projects/{}/issues",
        encode_project_id(&p.project_id)
    );
    let body = BodyBuilder::new()
        .req("title", &p.title)
        .opt("description", p.description)
        .opt("labels", p.labels)
        .opt("assignee_ids", p.assignee_ids)
        .opt("milestone_id", p.milestone_id)
        .opt("due_date", p.due_date)
        .opt("weight", p.weight)
        .build();
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
    let body = BodyBuilder::new()
        .opt("title", p.title)
        .opt("description", p.description)
        .opt("state_event", p.state_event)
        .opt("labels", p.labels)
        .opt("assignee_ids", p.assignee_ids)
        .opt("milestone_id", p.milestone_id)
        .opt("due_date", p.due_date)
        .opt("weight", p.weight)
        .build();
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
