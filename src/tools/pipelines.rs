use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{PaginationParams, QueryBuilder, encode_project_id};

// --------------------------------------------------------------------------
// List pipelines
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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

pub async fn pipeline_list(
    client: &GitlabClient,
    p: PipelineListParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines",
        encode_project_id(&p.project_id)
    );
    let params = QueryBuilder::new()
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
        .opt("name", p.name)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get a single pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get(
    client: &GitlabClient,
    p: PipelineGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get latest pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetLatestParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[serde(rename = "ref")]
    #[schemars(description = "Branch or tag name (defaults to the project default branch)")]
    pub ref_: Option<String>,
}

pub async fn pipeline_get_latest(
    client: &GitlabClient,
    p: PipelineGetLatestParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/latest",
        encode_project_id(&p.project_id)
    );
    let params = QueryBuilder::new().opt("ref", p.ref_).into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get pipeline variables
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetVariablesParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get_variables(
    client: &GitlabClient,
    p: PipelineGetVariablesParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}/variables",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.list(&path, &[]).await
}

// --------------------------------------------------------------------------
// Get pipeline test report
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetTestReportParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get_test_report(
    client: &GitlabClient,
    p: PipelineGetTestReportParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}/test_report",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get pipeline test report summary
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineGetTestReportSummaryParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_get_test_report_summary(
    client: &GitlabClient,
    p: PipelineGetTestReportSummaryParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}/test_report_summary",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Create a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
    let path = format!(
        "/api/v4/projects/{}/pipeline",
        encode_project_id(&p.project_id)
    );
    let mut body = json!({ "ref": p.ref_ });
    let obj = body.as_object_mut().unwrap();
    if let Some(v) = p.variables {
        obj.insert("variables".into(), json!(v));
    }
    if let Some(v) = p.inputs {
        obj.insert("inputs".into(), v);
    }
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Retry a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineRetryParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_retry(
    client: &GitlabClient,
    p: PipelineRetryParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}/retry",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Cancel a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineCancelParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_cancel(
    client: &GitlabClient,
    p: PipelineCancelParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}/cancel",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Delete a pipeline
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline ID")]
    pub pipeline_id: u64,
}

pub async fn pipeline_delete(
    client: &GitlabClient,
    p: PipelineDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipelines/{}",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Update pipeline metadata
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineUpdateMetadataParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/pipelines/{}/metadata",
        encode_project_id(&p.project_id),
        p.pipeline_id
    );
    client.put(&path, &json!({ "name": p.name })).await
}
