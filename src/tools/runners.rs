use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{PaginationParams, QueryBuilder, encode_namespace_id};

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
    #[schemars(
        description = "Filter runners by version prefix (e.g. \"15.0\", \"16.1.241\")"
    )]
    pub version_prefix: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn runners_list(client: &GitlabClient, p: RunnersListParams) -> ListResult {
    let params = QueryBuilder::new()
        .opt("type", p.runner_type)
        .opt("status", p.status)
        .opt("paused", p.paused)
        .multi("tag_list[]", p.tag_list)
        .opt("version_prefix", p.version_prefix)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list("/api/v4/runners", &params).await
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
    #[schemars(
        description = "Filter runners by version prefix (e.g. \"15.0\", \"16.1.241\")"
    )]
    pub version_prefix: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn runners_all_list(client: &GitlabClient, p: RunnersAllListParams) -> ListResult {
    let params = QueryBuilder::new()
        .opt("type", p.runner_type)
        .opt("status", p.status)
        .opt("paused", p.paused)
        .multi("tag_list[]", p.tag_list)
        .opt("version_prefix", p.version_prefix)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list("/api/v4/runners/all", &params).await
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
    #[schemars(
        description = "Machine system ID used to filter to a specific runner manager"
    )]
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
    let params = QueryBuilder::new()
        .opt("system_id", p.system_id)
        .opt("status", p.status)
        .opt("sort", p.sort)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
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
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
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
    #[schemars(
        description = "Filter runners by version prefix (e.g. \"15.0\", \"16.1.241\")"
    )]
    pub version_prefix: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn project_runners_list(
    client: &GitlabClient,
    p: ProjectRunnersListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/runners",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("type", p.runner_type)
        .opt("status", p.status)
        .opt("paused", p.paused)
        .multi("tag_list[]", p.tag_list)
        .opt("version_prefix", p.version_prefix)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// List group runners
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupRunnersListParams {
    #[schemars(description = "Group ID or URL-encoded path (e.g. 5 or \"mygroup\")")]
    pub group_id: String,
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
    #[schemars(
        description = "Filter runners by version prefix (e.g. \"15.0\", \"16.1.241\")"
    )]
    pub version_prefix: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn group_runners_list(client: &GitlabClient, p: GroupRunnersListParams) -> ListResult {
    let path = format!(
        "/api/v4/groups/{}/runners",
        encode_namespace_id(&p.group_id)
    );
    let params = QueryBuilder::new()
        .opt("status", p.status)
        .opt("paused", p.paused)
        .multi("tag_list[]", p.tag_list)
        .opt("version_prefix", p.version_prefix)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::client::GitlabClient;
    use crate::tools::PaginationParams;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn no_pagination() -> PaginationParams {
        PaginationParams {
            page: None,
            per_page: None,
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
                status: None,
                paused: None,
                tag_list: None,
                version_prefix: None,
                pagination: no_pagination(),
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id": 2}])),
            )
            .mount(&server)
            .await;

        let (items, _) = runners_all_list(
            &mock_client(&server),
            RunnersAllListParams {
                runner_type: None,
                status: None,
                paused: None,
                tag_list: None,
                version_prefix: None,
                pagination: no_pagination(),
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": 42,
                    "description": "my-runner",
                    "status": "online"
                })),
            )
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
                pagination: no_pagination(),
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
                pagination: no_pagination(),
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id": 5}])),
            )
            .mount(&server)
            .await;

        let (items, _) = project_runners_list(
            &mock_client(&server),
            ProjectRunnersListParams {
                project_id: "mygroup/myrepo".into(),
                runner_type: None,
                status: None,
                paused: None,
                tag_list: None,
                version_prefix: None,
                pagination: no_pagination(),
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
                status: None,
                paused: None,
                tag_list: None,
                version_prefix: None,
                pagination: no_pagination(),
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            )
            .mount(&server)
            .await;

        let (items, _) = group_runners_list(
            &mock_client(&server),
            GroupRunnersListParams {
                group_id: "mygroup/sub".into(),
                status: None,
                paused: None,
                tag_list: None,
                version_prefix: None,
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap();
        assert!(items.as_array().unwrap().is_empty());
    }
}
