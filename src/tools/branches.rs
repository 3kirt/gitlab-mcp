use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    PaginationParams, QueryBuilder, encode_path_segment, list_paginated, project_path,
};

// --------------------------------------------------------------------------
// List branches
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BranchesListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[schemars(description = "Return branches with names matching this re2 regular expression")]
    pub regex: Option<String>,
    #[schemars(description = "Return branches whose names contain the search string")]
    pub search: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn branches_list(client: &GitlabClient, p: BranchesListParams) -> ListResult {
    let path = format!("{}/repository/branches", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .opt("regex", p.regex)
        .opt("search", p.search);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get single branch
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BranchGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Branch name (slashes are URL-encoded automatically)")]
    pub branch: String,
}

pub async fn branch_get(client: &GitlabClient, p: BranchGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/branches/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.branch)
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create branch
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BranchCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "New branch name")]
    pub branch: String,
    #[serde(rename = "ref")]
    #[schemars(
        rename = "ref",
        description = "Source branch name or commit SHA to branch from"
    )]
    pub source_ref: String,
}

pub async fn branch_create(
    client: &GitlabClient,
    p: BranchCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/repository/branches", project_path(&p.project_id));
    let body = json!({
        "branch": p.branch,
        "ref": p.source_ref,
    });
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete branch
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BranchDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Branch name to delete (cannot delete default or protected branches)"
    )]
    pub branch: String,
}

pub async fn branch_delete(
    client: &GitlabClient,
    p: BranchDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "{}/repository/branches/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.branch)
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Delete all merged branches
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BranchesDeleteMergedParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
}

pub async fn branches_delete_merged(
    client: &GitlabClient,
    p: BranchesDeleteMergedParams,
) -> Result<(), GitlabError> {
    let path = format!("{}/repository/merged_branches", project_path(&p.project_id));
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

#[tool_router(router = tool_router_branches, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List branches for a GitLab project, sorted alphabetically. Optional filters: search (substring match) and regex (re2 regular expression). Paginate with page and per_page."
    )]
    async fn gitlab_branches_list(
        &self,
        Parameters(p): Parameters<BranchesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, branches_list, p, "branches")
    }

    #[tool(
        description = "Get a single GitLab branch by project and branch name. Returns commit details and protection status."
    )]
    async fn gitlab_branches_get(
        &self,
        Parameters(p): Parameters<BranchGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, branch_get, p, "branch")
    }

    #[tool(
        description = "Create a new branch in a GitLab project. Required: project_id, branch (new branch name), ref (source branch name or commit SHA)."
    )]
    async fn gitlab_branches_create(
        &self,
        Parameters(p): Parameters<BranchCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, branch_create, p, "branch")
    }

    #[tool(
        description = "Delete a GitLab branch by name. Cannot delete default or protected branches."
    )]
    async fn gitlab_branches_delete(
        &self,
        Parameters(p): Parameters<BranchDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, branch_delete, p, "branch")
    }

    #[tool(
        description = "Delete all branches in a GitLab project that have been merged into the default branch. Protected branches are excluded."
    )]
    async fn gitlab_branches_delete_merged(
        &self,
        Parameters(p): Parameters<BranchesDeleteMergedParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, branches_delete_merged, p, "merged branches")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        BranchCreateParams, BranchDeleteParams, BranchGetParams, branch_create, branch_delete,
        branch_get,
    };
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn branch_get_encodes_slash_in_branch_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v4/projects/42/repository/branches/feature%2Ffoo",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "name": "feature/foo" })),
            )
            .mount(&server)
            .await;

        let item = branch_get(
            &mock_client(&server),
            BranchGetParams {
                project_id: "42".into(),
                branch: "feature/foo".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["name"], "feature/foo");
    }

    #[tokio::test]
    async fn branch_delete_encodes_slash_in_branch_name() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(
                "/api/v4/projects/mygroup%2Fmyrepo/repository/branches/release%2F2026-01",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        branch_delete(
            &mock_client(&server),
            BranchDeleteParams {
                project_id: "mygroup/myrepo".into(),
                branch: "release/2026-01".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn branch_create_posts_branch_and_ref_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/repository/branches"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({ "name": "feature/new" })),
            )
            .mount(&server)
            .await;

        let item = branch_create(
            &mock_client(&server),
            BranchCreateParams {
                project_id: "42".into(),
                branch: "feature/new".into(),
                source_ref: "main".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["name"], "feature/new");

        let reqs = server.received_requests().await.unwrap();
        let body = reqs
            .iter()
            .find(|r| r.method == wiremock::http::Method::POST)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("POST request not found");
        assert_eq!(body["branch"], "feature/new");
        assert_eq!(body["ref"], "main");
        assert!(body.get("source_ref").is_none());
    }
}
