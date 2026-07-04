use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List project jobs
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Filter by one or more job states: \"created\", \"pending\", \"running\", \"failed\", \"success\", \"canceled\", \"skipped\", \"waiting_for_resource\", \"manual\""
    )]
    pub scope: Option<Vec<String>>,
    #[schemars(
        description = "Sort field: \"id\", \"name\", \"runner_id\", \"created_at\", \"started_at\", \"finished_at\", or \"erased_at\""
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\"")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn job_list(client: &GitlabClient, p: JobListParams) -> ListResult {
    let path = format!("{}/jobs", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .multi("scope[]", p.scope)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// List jobs for a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListForPipelineParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
    #[schemars(
        description = "Filter by one or more job states: \"created\", \"pending\", \"running\", \"failed\", \"success\", \"canceled\", \"skipped\", \"waiting_for_resource\", \"manual\""
    )]
    pub scope: Option<Vec<String>>,
    #[schemars(description = "If true, include retried jobs in addition to the latest attempt")]
    pub include_retried: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn job_list_for_pipeline(
    client: &GitlabClient,
    p: JobListForPipelineParams,
) -> ListResult {
    let path = format!(
        "{}/pipelines/{}/jobs",
        project_path(&p.project_id),
        p.pipeline_id
    );
    let qb = QueryBuilder::new()
        .multi("scope[]", p.scope)
        .opt("include_retried", p.include_retried);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// List bridge (trigger) jobs for a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListBridgesParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
    #[schemars(
        description = "Filter by one or more job states: \"created\", \"pending\", \"running\", \"failed\", \"success\", \"canceled\", \"skipped\", \"waiting_for_resource\", \"manual\""
    )]
    pub scope: Option<Vec<String>>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn job_list_bridges(client: &GitlabClient, p: JobListBridgesParams) -> ListResult {
    let path = format!(
        "{}/pipelines/{}/bridges",
        project_path(&p.project_id),
        p.pipeline_id
    );
    let qb = QueryBuilder::new().multi("scope[]", p.scope);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get a single job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_get(client: &GitlabClient, p: JobGetParams) -> Result<Value, GitlabError> {
    let path = format!("{}/jobs/{}", project_path(&p.project_id), p.job_id);
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get job trace (log)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobGetTraceParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_get_trace(
    client: &GitlabClient,
    p: JobGetTraceParams,
) -> Result<String, GitlabError> {
    let path = format!("{}/jobs/{}/trace", project_path(&p.project_id), p.job_id);
    client.get_text(&path, &[]).await
}

// --------------------------------------------------------------------------
// Cancel a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobCancelParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
    #[schemars(
        description = "If true, force-cancel a job already in \"canceling\" state (default: false)"
    )]
    pub force: Option<bool>,
}

pub async fn job_cancel(client: &GitlabClient, p: JobCancelParams) -> Result<Value, GitlabError> {
    let path = format!("{}/jobs/{}/cancel", project_path(&p.project_id), p.job_id);
    let body = BodyBuilder::new().opt("force", p.force).build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Retry a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobRetryParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_retry(client: &GitlabClient, p: JobRetryParams) -> Result<Value, GitlabError> {
    let path = format!("{}/jobs/{}/retry", project_path(&p.project_id), p.job_id);
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Erase a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobEraseParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_erase(client: &GitlabClient, p: JobEraseParams) -> Result<Value, GitlabError> {
    let path = format!("{}/jobs/{}/erase", project_path(&p.project_id), p.job_id);
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Play (trigger) a manual job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobPlayParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
    #[schemars(
        description = "Variable overrides for this run — array of objects with keys: key, value, variable_type (\"env_var\" or \"file\")"
    )]
    pub job_variables_attributes: Option<Vec<Value>>,
}

pub async fn job_play(client: &GitlabClient, p: JobPlayParams) -> Result<Value, GitlabError> {
    let path = format!("{}/jobs/{}/play", project_path(&p.project_id), p.job_id);
    let body = BodyBuilder::new()
        .opt("job_variables_attributes", p.job_variables_attributes)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_jobs, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List jobs for a GitLab project. Optional: scope (array of states to filter by), order_by, sort, page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_jobs_list(
        &self,
        Parameters(p): Parameters<JobListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, job_list, p, "jobs")
    }

    #[tool(
        description = "List jobs for a specific GitLab pipeline. Optional: scope (array of states), include_retried (include non-latest attempts), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_jobs_list_for_pipeline(
        &self,
        Parameters(p): Parameters<JobListForPipelineParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, job_list_for_pipeline, p, "pipeline jobs")
    }

    #[tool(
        description = "List bridge (downstream trigger) jobs for a GitLab pipeline. Optional: scope (array of states), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_jobs_list_bridges(
        &self,
        Parameters(p): Parameters<JobListBridgesParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, job_list_bridges, p, "pipeline bridges")
    }

    #[tool(
        description = "Get a single GitLab job by project ID and job ID. Returns full job metadata including stage, status, runner, timings, and artifacts.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_jobs_get(
        &self,
        Parameters(p): Parameters<JobGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, job_get, p, "job")
    }

    #[tool(
        description = "Get the raw log output (trace) of a GitLab job as plain text.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_jobs_get_trace(
        &self,
        Parameters(p): Parameters<JobGetTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_text!(self, job_get_trace, p, "getting", "job trace")
    }

    #[tool(
        description = "Cancel a running GitLab job. Optional: force (force-cancel a job already in \"canceling\" state).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_jobs_cancel(
        &self,
        Parameters(p): Parameters<JobCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, job_cancel, p, "canceling", "job")
    }

    #[tool(
        description = "Retry a failed or canceled GitLab job, creating a new job run.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_jobs_retry(
        &self,
        Parameters(p): Parameters<JobRetryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, job_retry, p, "retrying", "job")
    }

    #[tool(
        description = "Erase a GitLab job — removes the job log and artifacts. The job must be finished.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_jobs_erase(
        &self,
        Parameters(p): Parameters<JobEraseParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, job_erase, p, "erasing", "job")
    }

    #[tool(
        description = "Trigger a manual GitLab job. Optional: job_variables_attributes (array of {key, value, variable_type} objects to override job variables).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_jobs_play(
        &self,
        Parameters(p): Parameters<JobPlayParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, job_play, p, "triggering", "job")
    }
}
