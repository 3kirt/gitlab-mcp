use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, encode_path_segment,
    list_paginated,
};

// --------------------------------------------------------------------------
// List pipeline schedules
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulesListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Filter by scope: \"active\" or \"inactive\"")]
    pub scope: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn pipeline_schedules_list(
    client: &GitlabClient,
    p: PipelineSchedulesListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules",
        encode_namespace_id(&p.project_id)
    );
    let qb = QueryBuilder::new().opt("scope", p.scope);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get single pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_get(
    client: &GitlabClient,
    p: PipelineScheduleGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// List pipelines from schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulePipelinesListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(
        description = "Filter by status: \"created\", \"pending\", \"running\", \"failed\", \"success\", \"canceled\", \"skipped\", \"waiting_for_resource\", \"manual\""
    )]
    pub status: Option<String>,
    #[schemars(description = "Filter by scope")]
    pub scope: Option<String>,
    #[schemars(description = "Sort order: \"asc\" or \"desc\"")]
    pub sort: Option<String>,
    #[schemars(description = "Return pipelines updated after this date (ISO 8601)")]
    pub updated_after: Option<String>,
    #[schemars(description = "Return pipelines updated before this date (ISO 8601)")]
    pub updated_before: Option<String>,
    #[schemars(description = "Return pipelines created after this date (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return pipelines created before this date (ISO 8601)")]
    pub created_before: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn pipeline_schedule_pipelines_list(
    client: &GitlabClient,
    p: PipelineSchedulePipelinesListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/pipelines",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    let qb = QueryBuilder::new()
        .opt("status", p.status)
        .opt("scope", p.scope)
        .opt("sort", p.sort)
        .opt("updated_after", p.updated_after)
        .opt("updated_before", p.updated_before)
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Create pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Cron expression (e.g., \"0 0 * * *\")")]
    pub cron: String,
    #[schemars(description = "Schedule description")]
    pub description: String,
    #[serde(rename = "ref")]
    #[schemars(
        rename = "ref",
        description = "Branch or tag name to run the schedule on"
    )]
    pub ref_name: String,
    #[schemars(description = "Whether the schedule is active (default: true)")]
    pub active: Option<bool>,
    #[schemars(description = "Cron timezone (e.g., \"UTC\", \"America/Los_Angeles\")")]
    pub cron_timezone: Option<String>,
}

pub async fn pipeline_schedule_create(
    client: &GitlabClient,
    p: PipelineScheduleCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules",
        encode_namespace_id(&p.project_id)
    );
    let body = BodyBuilder::new()
        .req("cron", p.cron)
        .req("description", p.description)
        .req("ref", p.ref_name)
        .opt("active", p.active)
        .opt("cron_timezone", p.cron_timezone)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(description = "Cron expression")]
    pub cron: Option<String>,
    #[schemars(description = "Schedule description")]
    pub description: Option<String>,
    #[serde(rename = "ref")]
    #[schemars(rename = "ref", description = "Branch or tag name")]
    pub ref_name: Option<String>,
    #[schemars(description = "Whether the schedule is active")]
    pub active: Option<bool>,
    #[schemars(description = "Cron timezone")]
    pub cron_timezone: Option<String>,
}

pub async fn pipeline_schedule_update(
    client: &GitlabClient,
    p: PipelineScheduleUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    let body = BodyBuilder::new()
        .opt("cron", p.cron)
        .opt("description", p.description)
        .opt("ref", p.ref_name)
        .opt("active", p.active)
        .opt("cron_timezone", p.cron_timezone)
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Take ownership of pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleTakeOwnershipParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_take_ownership(
    client: &GitlabClient,
    p: PipelineScheduleTakeOwnershipParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/take_ownership",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Play (run immediately) pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulePlayParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_play(
    client: &GitlabClient,
    p: PipelineSchedulePlayParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/play",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Delete pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_delete(
    client: &GitlabClient,
    p: PipelineScheduleDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Pipeline schedule variables
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(description = "Variable key")]
    pub key: String,
    #[schemars(description = "Variable value")]
    pub value: String,
    #[schemars(description = "Variable type: \"env_var\" (default) or \"file\"")]
    pub variable_type: Option<String>,
}

pub async fn pipeline_schedule_variable_create(
    client: &GitlabClient,
    p: PipelineScheduleVariableCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/variables",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id
    );
    let body = BodyBuilder::new()
        .req("key", p.key)
        .req("value", p.value)
        .opt("variable_type", p.variable_type)
        .build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(description = "Variable key")]
    pub key: String,
}

pub async fn pipeline_schedule_variable_get(
    client: &GitlabClient,
    p: PipelineScheduleVariableGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/variables/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id,
        encode_path_segment(&p.key)
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(description = "Variable key")]
    pub key: String,
    #[schemars(description = "Variable value")]
    pub value: String,
    #[schemars(description = "Variable type: \"env_var\" (default) or \"file\"")]
    pub variable_type: Option<String>,
}

pub async fn pipeline_schedule_variable_update(
    client: &GitlabClient,
    p: PipelineScheduleVariableUpdateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/variables/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id,
        encode_path_segment(&p.key)
    );
    let body = BodyBuilder::new()
        .req("value", p.value)
        .opt("variable_type", p.variable_type)
        .build();
    client.put(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
    #[schemars(description = "Variable key")]
    pub key: String,
}

pub async fn pipeline_schedule_variable_delete(
    client: &GitlabClient,
    p: PipelineScheduleVariableDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/pipeline_schedules/{}/variables/{}",
        encode_namespace_id(&p.project_id),
        p.pipeline_schedule_id,
        encode_path_segment(&p.key)
    );
    client.delete(&path).await
}
