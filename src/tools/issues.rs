use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, paginate,
    unwrap_404_as_empty_array,
};

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
    #[schemars(
        description = "Return only issues created after this datetime (ISO 8601, e.g. \"2024-01-01T00:00:00Z\")"
    )]
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

pub async fn issues_list(client: &GitlabClient, p: IssuesListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues",
        encode_namespace_id(&p.project_id)
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
    paginate(
        client,
        &path,
        &params,
        p.pagination.fetch_all.unwrap_or(false),
    )
    .await
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
    let pid = encode_namespace_id(&p.project_id);
    let iid = p.issue_iid;
    let issue_path = format!("/api/v4/projects/{pid}/issues/{iid}");
    let links_path = format!("/api/v4/projects/{pid}/issues/{iid}/links");
    let closed_by_path = format!("/api/v4/projects/{pid}/issues/{iid}/closed_by");
    let (mut issue, links, closed_by) = tokio::try_join!(
        client.get(&issue_path),
        async { unwrap_404_as_empty_array(client.get(&links_path).await) },
        async { unwrap_404_as_empty_array(client.get(&closed_by_path).await) },
    )?;
    issue["linked_issues"] = links;
    issue["closed_by"] = closed_by;
    Ok(issue)
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
        encode_namespace_id(&p.project_id)
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
        encode_namespace_id(&p.project_id),
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
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// List issue links
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueLinksListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
}

pub async fn issue_links_list(client: &GitlabClient, p: IssueLinksListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/links",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    client.list(&path, &[]).await
}

// --------------------------------------------------------------------------
// Get single issue link
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueLinkGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Issue link relationship ID (issue_link_id from the list response)")]
    pub issue_link_id: u64,
}

pub async fn issue_link_get(
    client: &GitlabClient,
    p: IssueLinkGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/links/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.issue_link_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create issue link
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueLinkCreateParams {
    #[schemars(description = "Project ID or URL-encoded path of the source issue")]
    pub project_id: String,
    #[schemars(description = "Source issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Target project ID or URL-encoded path")]
    pub target_project_id: String,
    #[schemars(description = "Target issue internal ID (IID)")]
    pub target_issue_iid: u64,
    #[schemars(
        description = "Relationship type: \"relates_to\" (default), \"blocks\", or \"is_blocked_by\""
    )]
    pub link_type: Option<String>,
}

pub async fn issue_link_create(
    client: &GitlabClient,
    p: IssueLinkCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/links",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let body = BodyBuilder::new()
        .req("target_project_id", &p.target_project_id)
        .req("target_issue_iid", p.target_issue_iid)
        .opt("link_type", p.link_type)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete issue link
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueLinkDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Issue link relationship ID (issue_link_id from the list response)")]
    pub issue_link_id: u64,
}

pub async fn issue_link_delete(
    client: &GitlabClient,
    p: IssueLinkDeleteParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/links/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.issue_link_id
    );
    client.delete_json(&path).await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{IssueGetParams, issue_get};
    use crate::client::GitlabClient;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn issue_json(iid: u64) -> serde_json::Value {
        serde_json::json!({
            "id": iid * 100,
            "iid": iid,
            "project_id": 1,
            "title": format!("Issue {iid}"),
            "state": "opened",
            "web_url": format!("https://gitlab.example.com/p/-/issues/{iid}"),
        })
    }

    #[tokio::test]
    async fn issue_get_embeds_links_and_closed_by() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(7)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo/issues/7/links"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 99, "iid": 8, "link_type": "blocks", "issue_link_id": 12 }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo/issues/7/closed_by"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 555, "iid": 3, "title": "Fix it", "state": "merged", "project_id": 1 }
            ])))
            .mount(&server)
            .await;

        let item = issue_get(
            &mock_client(&server),
            IssueGetParams {
                project_id: "mygroup/myrepo".into(),
                issue_iid: 7,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["iid"], 7);
        assert_eq!(item["linked_issues"][0]["link_type"], "blocks");
        assert_eq!(item["closed_by"][0]["iid"], 3);
    }

    #[tokio::test]
    async fn issue_get_tolerates_missing_embed_endpoints() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(4)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/4/links"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/4/closed_by"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let item = issue_get(
            &mock_client(&server),
            IssueGetParams {
                project_id: "p".into(),
                issue_iid: 4,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["linked_issues"], serde_json::json!([]));
        assert_eq!(item["closed_by"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn issue_get_propagates_404_for_issue_itself() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/999"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;
        // Mock the embed endpoints so a concurrent fetch doesn't 404 with no route.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/999/links"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/999/closed_by"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let err = issue_get(
            &mock_client(&server),
            IssueGetParams {
                project_id: "p".into(),
                issue_iid: 999,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    #[tokio::test]
    async fn issue_get_propagates_500_from_embed() {
        // A non-404/403 error on a supplemental fetch must surface, not be silently swallowed.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(5)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/5/links"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/issues/5/closed_by"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let err = issue_get(
            &mock_client(&server),
            IssueGetParams {
                project_id: "p".into(),
                issue_iid: 5,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::client::GitlabError::Api { status, .. } if status.as_u16() == 500)
        );
    }
}
