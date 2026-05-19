use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_project_id};

// --------------------------------------------------------------------------
// List project jobs
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
    let path = format!("/api/v4/projects/{}/jobs", encode_project_id(&p.project_id));
    let params = QueryBuilder::new()
        .multi("scope[]", p.scope)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// List jobs for a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListForPipelineParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/pipelines/{}/jobs",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    let params = QueryBuilder::new()
        .multi("scope[]", p.scope)
        .opt("include_retried", p.include_retried)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// List bridge (trigger) jobs for a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobListBridgesParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/pipelines/{}/bridges",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    let params = QueryBuilder::new()
        .multi("scope[]", p.scope)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get a single job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_get(client: &GitlabClient, p: JobGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}",
        encode_project_id(&p.project_id),
        p.job_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get job trace (log)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobGetTraceParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_get_trace(
    client: &GitlabClient,
    p: JobGetTraceParams,
) -> Result<String, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}/trace",
        encode_project_id(&p.project_id),
        p.job_id
    );
    client.get_text(&path, &[]).await
}

// --------------------------------------------------------------------------
// Cancel a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobCancelParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
    #[schemars(
        description = "If true, force-cancel a job already in \"canceling\" state (default: false)"
    )]
    pub force: Option<bool>,
}

pub async fn job_cancel(client: &GitlabClient, p: JobCancelParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}/cancel",
        encode_project_id(&p.project_id),
        p.job_id
    );
    let body = BodyBuilder::new().opt("force", p.force).build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Retry a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobRetryParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_retry(client: &GitlabClient, p: JobRetryParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}/retry",
        encode_project_id(&p.project_id),
        p.job_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Erase a job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobEraseParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
}

pub async fn job_erase(client: &GitlabClient, p: JobEraseParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}/erase",
        encode_project_id(&p.project_id),
        p.job_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Play (trigger) a manual job
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JobPlayParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Job ID")]
    pub job_id: u64,
    #[schemars(
        description = "Variable overrides for this run — array of objects with keys: key, value, variable_type (\"env_var\" or \"file\")"
    )]
    pub job_variables_attributes: Option<Vec<Value>>,
}

pub async fn job_play(client: &GitlabClient, p: JobPlayParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/jobs/{}/play",
        encode_project_id(&p.project_id),
        p.job_id
    );
    let body = BodyBuilder::new()
        .opt("job_variables_attributes", p.job_variables_attributes)
        .build();
    client.post(&path, &body).await
}
