use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{QueryBuilder, project_path};

// --------------------------------------------------------------------------
// Get single project
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectGetParams {
    #[schemars(
        description = "Project ID (numeric) or URL-encoded namespace path (e.g. \"mygroup/myrepo\")"
    )]
    pub project_id: String,
    #[schemars(description = "Include project statistics (requires Reporter role or higher)")]
    pub statistics: Option<bool>,
}

pub async fn project_get(client: &GitlabClient, p: ProjectGetParams) -> Result<Value, GitlabError> {
    let proj = project_path(&p.project_id);
    let params = QueryBuilder::new()
        .opt("statistics", p.statistics)
        .into_params();
    client.get_with_params(&proj, &params).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_projects, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "Get a GitLab project by ID or namespace path. project_id accepts a numeric ID (e.g. \"42\") or a full namespace path (e.g. \"mygroup/myrepo\"). Optional: statistics=true to include commit/storage counts (requires Reporter role or higher). Returns core project details: id, name, path, path_with_namespace, description, visibility, default_branch, web_url, http_url_to_repo, namespace, created_at, and feature settings."
    )]
    async fn gitlab_projects_get(
        &self,
        Parameters(p): Parameters<ProjectGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, project_get, p, "project")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{ProjectGetParams, project_get};
    use crate::test_util::mock_client;

    fn project_json(id: u64, name: &str, path_str: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": name,
            "path": path_str,
            "path_with_namespace": format!("mygroup/{path_str}"),
            "description": null,
            "visibility": "private",
            "default_branch": "main",
            "web_url": format!("https://gitlab.example.com/mygroup/{path_str}"),
            "http_url_to_repo": format!("https://gitlab.example.com/mygroup/{path_str}.git"),
            "namespace": { "id": 10, "name": "mygroup", "path": "mygroup" },
            "created_at": "2024-01-01T00:00:00.000Z"
        })
    }

    #[tokio::test]
    async fn project_get_returns_project_by_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(project_json(1, "My Repo", "myrepo")),
            )
            .mount(&server)
            .await;

        let item = project_get(
            &mock_client(&server),
            ProjectGetParams {
                project_id: "mygroup/myrepo".into(),
                statistics: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["name"], "My Repo");
        assert_eq!(item["path"], "myrepo");
    }

    #[tokio::test]
    async fn project_get_returns_project_by_numeric_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(project_json(42, "Numeric", "numeric")),
            )
            .mount(&server)
            .await;

        let item = project_get(
            &mock_client(&server),
            ProjectGetParams {
                project_id: "42".into(),
                statistics: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["id"], 42);
    }

    #[tokio::test]
    async fn project_get_forwards_statistics_param() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo"))
            .and(query_param("statistics", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json({
                let mut p = project_json(1, "My Repo", "myrepo");
                p["statistics"] = serde_json::json!({ "commit_count": 10 });
                p
            }))
            .mount(&server)
            .await;

        let item = project_get(
            &mock_client(&server),
            ProjectGetParams {
                project_id: "mygroup/myrepo".into(),
                statistics: Some(true),
            },
        )
        .await
        .unwrap();

        assert_eq!(item["statistics"]["commit_count"], 10);
    }

    #[tokio::test]
    async fn project_get_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/ghost%2Frepo"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = project_get(
            &mock_client(&server),
            ProjectGetParams {
                project_id: "ghost/repo".into(),
                statistics: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
