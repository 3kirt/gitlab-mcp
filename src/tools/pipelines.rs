use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List pipelines
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Filter by scope: \"created\", \"pending\", \"running\", \"failed\", \"success\", \"canceled\", \"skipped\", \"waiting_for_resource\", or \"manual\""
    )]
    pub scope: Option<String>,
    #[schemars(description = "Filter by status (same values as scope)")]
    pub status: Option<String>,
    #[schemars(
        description = "Filter by source: \"push\", \"web\", \"trigger\", \"schedule\", \"api\", \"merge_request_event\", etc."
    )]
    pub source: Option<String>,
    #[serde(rename = "ref")]
    #[schemars(description = "Filter by branch or tag name")]
    pub ref_: Option<String>,
    #[schemars(description = "Filter by commit SHA")]
    pub sha: Option<String>,
    #[schemars(description = "If true, filter pipelines with YAML errors only")]
    pub yaml_errors: Option<bool>,
    #[schemars(description = "Filter by triggering username")]
    pub username: Option<String>,
    #[schemars(
        description = "Return pipelines updated after this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub updated_after: Option<String>,
    #[schemars(
        description = "Return pipelines updated before this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub updated_before: Option<String>,
    #[schemars(
        description = "Return pipelines created after this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub created_after: Option<String>,
    #[schemars(
        description = "Return pipelines created before this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub created_before: Option<String>,
    #[schemars(
        description = "Sort field: \"id\", \"status\", \"ref\", \"updated_at\", or \"user_id\" (default: \"id\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[schemars(description = "Filter by pipeline name")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn pipeline_list(client: &GitlabClient, p: PipelineListParams) -> ListResult {
    let path = format!("{}/pipelines", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .opt("scope", p.scope)
        .opt("status", p.status)
        .opt("source", p.source)
        .opt("ref", p.ref_)
        .opt("sha", p.sha)
        .opt("yaml_errors", p.yaml_errors)
        .opt("username", p.username)
        .opt("updated_after", p.updated_after)
        .opt("updated_before", p.updated_before)
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("name", p.name);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get a single pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get(
    client: &GitlabClient,
    p: PipelineGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get latest pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetLatestParams {
    pub project_id: crate::tools::ProjectId,
    #[serde(rename = "ref")]
    #[schemars(description = "Branch or tag name (defaults to the project default branch)")]
    pub ref_: Option<String>,
}

pub async fn pipeline_get_latest(
    client: &GitlabClient,
    p: PipelineGetLatestParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/pipelines/latest", project_path(&p.project_id));
    let params = QueryBuilder::new().opt("ref", p.ref_).into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Get pipeline variables
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetVariablesParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn pipeline_get_variables(
    client: &GitlabClient,
    p: PipelineGetVariablesParams,
) -> ListResult {
    let path = format!(
        "{}/pipelines/{}/variables",
        project_path(&p.project_id),
        p.pipeline_id
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// Get pipeline test report
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetTestReportParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get_test_report(
    client: &GitlabClient,
    p: PipelineGetTestReportParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}/test_report",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get pipeline test report summary
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetTestReportSummaryParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get_test_report_summary(
    client: &GitlabClient,
    p: PipelineGetTestReportSummaryParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}/test_report_summary",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineCreateParams {
    pub project_id: crate::tools::ProjectId,
    #[serde(rename = "ref")]
    #[schemars(description = "Branch name, tag, or commit SHA to run the pipeline on")]
    pub ref_: String,
    #[schemars(
        description = "Pipeline variables — array of objects with keys: key, value, variable_type (\"env_var\" or \"file\")"
    )]
    pub variables: Option<Vec<Value>>,
    #[schemars(description = "Pipeline input values (for pipelines that declare inputs)")]
    pub inputs: Option<Value>,
}

pub async fn pipeline_create(
    client: &GitlabClient,
    p: PipelineCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/pipeline", project_path(&p.project_id));
    let body = BodyBuilder::new()
        .req("ref", &p.ref_)
        .opt("variables", p.variables)
        .opt("inputs", p.inputs)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Retry a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineRetryParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_retry(
    client: &GitlabClient,
    p: PipelineRetryParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}/retry",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Cancel a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineCancelParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_cancel(
    client: &GitlabClient,
    p: PipelineCancelParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}/cancel",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Delete a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineDeleteParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_delete(
    client: &GitlabClient,
    p: PipelineDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "{}/pipelines/{}",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Update pipeline metadata
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineUpdateMetadataParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
    #[schemars(description = "New pipeline name")]
    pub name: String,
}

pub async fn pipeline_update_metadata(
    client: &GitlabClient,
    p: PipelineUpdateMetadataParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipelines/{}/metadata",
        project_path(&p.project_id),
        p.pipeline_id
    );
    client.put(&path, &json!({ "name": p.name })).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_pipelines, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List pipelines for a GitLab project. Optional filters: scope, status, source, ref, sha, yaml_errors, username, updated_after/before, created_after/before, order_by, sort, name. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_list(
        &self,
        Parameters(p): Parameters<PipelineListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, pipeline_list, p, "pipelines")
    }

    #[tool(
        description = "Get a single GitLab pipeline by project ID and pipeline ID.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_get(
        &self,
        Parameters(p): Parameters<PipelineGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipeline_get, p, "pipeline")
    }

    #[tool(
        description = "Get the latest pipeline for a GitLab project. Optional: ref (branch or tag name; defaults to project default branch).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_get_latest(
        &self,
        Parameters(p): Parameters<PipelineGetLatestParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipeline_get_latest, p, "latest pipeline")
    }

    #[tool(
        description = "List variables defined on a specific GitLab pipeline run. Returns key/value pairs used when the pipeline was triggered. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_get_variables(
        &self,
        Parameters(p): Parameters<PipelineGetVariablesParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, pipeline_get_variables, p, "pipeline variables")
    }

    #[tool(
        description = "Get the full test report for a GitLab pipeline, including suite and case details with pass/fail/error counts.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_get_test_report(
        &self,
        Parameters(p): Parameters<PipelineGetTestReportParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipeline_get_test_report, p, "pipeline test report")
    }

    #[tool(
        description = "Get the test report summary for a GitLab pipeline — total counts only without per-case details.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_pipelines_get_test_report_summary(
        &self,
        Parameters(p): Parameters<PipelineGetTestReportSummaryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipeline_get_test_report_summary,
            p,
            "pipeline test report summary"
        )
    }

    #[tool(
        description = "Create (trigger) a new GitLab pipeline. Required: project_id, ref (branch/tag/SHA). Optional: variables (array of {key, value, variable_type} objects), inputs.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_pipelines_create(
        &self,
        Parameters(p): Parameters<PipelineCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, pipeline_create, p, "pipeline")
    }

    #[tool(
        description = "Retry all failed and canceled jobs in a GitLab pipeline, creating a new pipeline run.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_pipelines_retry(
        &self,
        Parameters(p): Parameters<PipelineRetryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, pipeline_retry, p, "retrying", "pipeline")
    }

    #[tool(
        description = "Cancel all running jobs in a GitLab pipeline.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_pipelines_cancel(
        &self,
        Parameters(p): Parameters<PipelineCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, pipeline_cancel, p, "canceling", "pipeline")
    }

    #[tool(
        description = "Delete a GitLab pipeline and all its jobs. Requires at least Maintainer role. This action is permanent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_pipelines_delete(
        &self,
        Parameters(p): Parameters<PipelineDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, pipeline_delete, p, "pipeline")
    }

    #[tool(
        description = "Update the name of a GitLab pipeline. Required: project_id, pipeline_id, name (new pipeline name).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_pipelines_update_metadata(
        &self,
        Parameters(p): Parameters<PipelineUpdateMetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, pipeline_update_metadata, p, "pipeline metadata")
    }
}
