use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{PaginationParams, QueryBuilder, group_path, list_paginated, project_path};

// --------------------------------------------------------------------------
// Shared list filters
//
// The four list endpoints (user runners, all runners, project runners, group
// runners) share the same status/paused/tag_list/version_prefix shape. The
// `type` filter is only valid where instance/group/project runners are
// distinguishable; group_runners_list drops it.
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnerListFilters {
    #[schemars(
        description = "Filter by runner status: \"online\", \"offline\", \"stale\", or \"never_contacted\""
    )]
    pub status: Option<String>,
    #[schemars(
        description = "Filter by job acceptance: true returns runners that do not accept new jobs"
    )]
    pub paused: Option<bool>,
    #[schemars(description = "Filter by runner tags; all listed tags must match")]
    pub tag_list: Option<Vec<String>>,
    #[schemars(description = "Filter runners by version prefix (e.g. \"15.0\", \"16.1.241\")")]
    pub version_prefix: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Build the shared runner list query, returning the pagination fields
/// separately so the caller can drive [`list_paginated`].
fn runner_query(
    runner_type: Option<String>,
    filters: RunnerListFilters,
) -> (QueryBuilder, PaginationParams) {
    let qb = QueryBuilder::new()
        .opt("type", runner_type)
        .opt("status", filters.status)
        .opt("paused", filters.paused)
        .multi("tag_list[]", filters.tag_list)
        .opt("version_prefix", filters.version_prefix);
    (qb, filters.pagination)
}

// --------------------------------------------------------------------------
// List runners (current user)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnersListParams {
    #[serde(rename = "type")]
    #[schemars(
        rename = "type",
        description = "Filter by runner type: \"instance_type\", \"group_type\", or \"project_type\""
    )]
    pub runner_type: Option<String>,
    #[serde(flatten)]
    pub filters: RunnerListFilters,
}

pub async fn runners_list(client: &GitlabClient, p: RunnersListParams) -> ListResult {
    let (qb, pagination) = runner_query(p.runner_type, p.filters);
    list_paginated(client, "/api/v4/runners", qb, pagination).await
}

// --------------------------------------------------------------------------
// List all runners (admin)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnersAllListParams {
    #[serde(rename = "type")]
    #[schemars(
        rename = "type",
        description = "Filter by runner type: \"instance_type\", \"group_type\", or \"project_type\""
    )]
    pub runner_type: Option<String>,
    #[serde(flatten)]
    pub filters: RunnerListFilters,
}

pub async fn runners_all_list(client: &GitlabClient, p: RunnersAllListParams) -> ListResult {
    let (qb, pagination) = runner_query(p.runner_type, p.filters);
    list_paginated(client, "/api/v4/runners/all", qb, pagination).await
}

// --------------------------------------------------------------------------
// Get runner details
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnerGetParams {
    #[schemars(description = "Runner ID")]
    pub id: u64,
}

pub async fn runner_get(client: &GitlabClient, p: RunnerGetParams) -> Result<Value, GitlabError> {
    let path = format!("/api/v4/runners/{}", p.id);
    client.get(&path).await
}

// --------------------------------------------------------------------------
// List runner jobs
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnerJobsListParams {
    #[schemars(description = "Runner ID")]
    pub id: u64,
    #[schemars(description = "Machine system ID used to filter to a specific runner manager")]
    pub system_id: Option<String>,
    #[schemars(
        description = "Filter by job status: \"running\", \"success\", \"failed\", or \"canceled\""
    )]
    pub status: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn runner_jobs_list(client: &GitlabClient, p: RunnerJobsListParams) -> ListResult {
    let path = format!("/api/v4/runners/{}/jobs", p.id);
    let qb = QueryBuilder::new()
        .opt("system_id", p.system_id)
        .opt("status", p.status)
        .opt("sort", p.sort);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// List runner managers
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunnerManagersListParams {
    #[schemars(description = "Runner ID")]
    pub id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn runner_managers_list(
    client: &GitlabClient,
    p: RunnerManagersListParams,
) -> ListResult {
    let path = format!("/api/v4/runners/{}/managers", p.id);
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// List project runners
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectRunnersListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[serde(rename = "type")]
    #[schemars(
        rename = "type",
        description = "Filter by runner type: \"instance_type\", \"group_type\", or \"project_type\""
    )]
    pub runner_type: Option<String>,
    #[serde(flatten)]
    pub filters: RunnerListFilters,
}

pub async fn project_runners_list(
    client: &GitlabClient,
    p: ProjectRunnersListParams,
) -> ListResult {
    let path = format!("{}/runners", project_path(&p.project_id));
    let (qb, pagination) = runner_query(p.runner_type, p.filters);
    list_paginated(client, &path, qb, pagination).await
}

// --------------------------------------------------------------------------
// List group runners
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupRunnersListParams {
    #[schemars(description = "Group ID or URL-encoded path (e.g. 5 or \"mygroup\")")]
    pub group_id: String,
    #[serde(flatten)]
    pub filters: RunnerListFilters,
}

pub async fn group_runners_list(client: &GitlabClient, p: GroupRunnersListParams) -> ListResult {
    let path = format!("{}/runners", group_path(&p.group_id));
    // Group runners endpoint doesn't accept a `type` filter — pass None.
    let (qb, pagination) = runner_query(None, p.filters);
    list_paginated(client, &path, qb, pagination).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_runners, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List runners available to the current authenticated user. Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list(
        &self,
        Parameters(p): Parameters<RunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners_list, p, "runners")
    }

    #[tool(
        description = "List all runners registered on the GitLab instance (administrators only). Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_all_list(
        &self,
        Parameters(p): Parameters<RunnersAllListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners_all_list, p, "runners")
    }

    #[tool(
        description = "Get details of a single GitLab runner by ID. Returns architecture, description, ip_address, status, tag_list, version, platform, projects, and more."
    )]
    async fn gitlab_runners_get(
        &self,
        Parameters(p): Parameters<RunnerGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, runner_get, p, "runner")
    }

    #[tool(
        description = "List jobs processed by a specific GitLab runner. Optional filters: system_id (runner manager), status (\"running\", \"success\", \"failed\", \"canceled\"), sort (\"asc\" or \"desc\"). Paginate with page and per_page."
    )]
    async fn gitlab_runners_jobs_list(
        &self,
        Parameters(p): Parameters<RunnerJobsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runner_jobs_list, p, "runner jobs")
    }

    #[tool(
        description = "List runner managers (individual machines) registered under a GitLab runner. Returns system_id, version, platform, architecture, ip_address, status, and last contact time. Paginate with page and per_page."
    )]
    async fn gitlab_runners_managers_list(
        &self,
        Parameters(p): Parameters<RunnerManagersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runner_managers_list, p, "runner managers")
    }

    #[tool(
        description = "List runners available to a GitLab project. project_id accepts a numeric ID or namespace path. Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list_for_project(
        &self,
        Parameters(p): Parameters<ProjectRunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, project_runners_list, p, "project runners")
    }

    #[tool(
        description = "List runners available to a GitLab group. group_id accepts a numeric ID or namespace path. Optional filters: status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list_for_group(
        &self,
        Parameters(p): Parameters<GroupRunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, group_runners_list, p, "group runners")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_util::mock_client;
    use crate::tools::PaginationParams;

    fn no_filters() -> RunnerListFilters {
        RunnerListFilters {
            status: None,
            paused: None,
            tag_list: None,
            version_prefix: None,
            pagination: PaginationParams {
                page: None,
                per_page: None,
                fetch_all: None,
            },
        }
    }

    #[tokio::test]
    async fn runners_list_returns_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{"id": 1, "status": "online"}])),
            )
            .mount(&server)
            .await;

        let (items, _) = runners_list(
            &mock_client(&server),
            RunnersListParams {
                runner_type: None,
                filters: no_filters(),
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["id"], 1);
    }

    #[tokio::test]
    async fn runners_all_list_hits_all_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners/all"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id": 2}])))
            .mount(&server)
            .await;

        let (items, _) = runners_all_list(
            &mock_client(&server),
            RunnersAllListParams {
                runner_type: None,
                filters: no_filters(),
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["id"], 2);
    }

    #[tokio::test]
    async fn runner_get_returns_runner_details() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "description": "my-runner",
                "status": "online"
            })))
            .mount(&server)
            .await;

        let runner = runner_get(&mock_client(&server), RunnerGetParams { id: 42 })
            .await
            .unwrap();
        assert_eq!(runner["id"], 42);
        assert_eq!(runner["description"], "my-runner");
    }

    #[tokio::test]
    async fn runner_get_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners/99"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "404 Not Found"
            })))
            .mount(&server)
            .await;

        let err = runner_get(&mock_client(&server), RunnerGetParams { id: 99 })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            GitlabError::Api { status, .. } if status == reqwest::StatusCode::NOT_FOUND
        ));
    }

    #[tokio::test]
    async fn runner_jobs_list_sends_status_filter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners/7/jobs"))
            .and(query_param("status", "running"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{"id": 100, "status": "running"}])),
            )
            .mount(&server)
            .await;

        let (items, _) = runner_jobs_list(
            &mock_client(&server),
            RunnerJobsListParams {
                id: 7,
                system_id: None,
                status: Some("running".into()),
                sort: None,
                pagination: PaginationParams {
                    page: None,
                    per_page: None,
                    fetch_all: None,
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["status"], "running");
    }

    #[tokio::test]
    async fn runner_managers_list_hits_correct_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/runners/42/managers"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{"id": 1, "system_id": "s_abc123"}])),
            )
            .mount(&server)
            .await;

        let (items, _) = runner_managers_list(
            &mock_client(&server),
            RunnerManagersListParams {
                id: 42,
                pagination: PaginationParams {
                    page: None,
                    per_page: None,
                    fetch_all: None,
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["system_id"], "s_abc123");
    }

    #[tokio::test]
    async fn project_runners_list_encodes_namespace_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo/runners"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id": 5}])))
            .mount(&server)
            .await;

        let (items, _) = project_runners_list(
            &mock_client(&server),
            ProjectRunnersListParams {
                project_id: "mygroup/myrepo".into(),
                runner_type: None,
                filters: no_filters(),
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["id"], 5);
    }

    #[tokio::test]
    async fn group_runners_list_hits_correct_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/5/runners"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{"id": 3, "status": "offline"}])),
            )
            .mount(&server)
            .await;

        let (items, _) = group_runners_list(
            &mock_client(&server),
            GroupRunnersListParams {
                group_id: "5".into(),
                filters: no_filters(),
            },
        )
        .await
        .unwrap();
        assert_eq!(items[0]["id"], 3);
    }

    #[tokio::test]
    async fn group_runners_list_encodes_group_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup%2Fsub/runners"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let (items, _) = group_runners_list(
            &mock_client(&server),
            GroupRunnersListParams {
                group_id: "mygroup/sub".into(),
                filters: no_filters(),
            },
        )
        .await
        .unwrap();
        assert!(items.as_array().unwrap().is_empty());
    }
}
