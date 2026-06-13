use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_path_segment, list_paginated, project_path,
};

// --------------------------------------------------------------------------
// List pipeline schedules
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulesListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Filter by scope: \"active\" or \"inactive\"")]
    pub scope: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn pipeline_schedules_list(
    client: &GitlabClient,
    p: PipelineSchedulesListParams,
) -> ListResult {
    let path = format!("{}/pipeline_schedules", project_path(&p.project_id));
    let qb = QueryBuilder::new().opt("scope", p.scope);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get single pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_get(
    client: &GitlabClient,
    p: PipelineScheduleGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipeline_schedules/{}",
        project_path(&p.project_id),
        p.pipeline_schedule_id
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// List pipelines from schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulePipelinesListParams {
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}/pipelines",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/pipeline_schedules", project_path(&p.project_id));
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_take_ownership(
    client: &GitlabClient,
    p: PipelineScheduleTakeOwnershipParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipeline_schedules/{}/take_ownership",
        project_path(&p.project_id),
        p.pipeline_schedule_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Play (run immediately) pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineSchedulePlayParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_play(
    client: &GitlabClient,
    p: PipelineSchedulePlayParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/pipeline_schedules/{}/play",
        project_path(&p.project_id),
        p.pipeline_schedule_id
    );
    client.post(&path, &json!({})).await
}

// --------------------------------------------------------------------------
// Delete pipeline schedule
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleDeleteParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Pipeline schedule ID")]
    pub pipeline_schedule_id: u64,
}

pub async fn pipeline_schedule_delete(
    client: &GitlabClient,
    p: PipelineScheduleDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "{}/pipeline_schedules/{}",
        project_path(&p.project_id),
        p.pipeline_schedule_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Pipeline schedule variables
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableCreateParams {
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}/variables",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}/variables/{}",
        project_path(&p.project_id),
        p.pipeline_schedule_id,
        encode_path_segment(&p.key)
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PipelineScheduleVariableUpdateParams {
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}/variables/{}",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
        "{}/pipeline_schedules/{}/variables/{}",
        project_path(&p.project_id),
        p.pipeline_schedule_id,
        encode_path_segment(&p.key)
    );
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

#[tool_router(router = tool_router_pipeline_schedules, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List pipeline schedules for a GitLab project. Optional: scope (\"active\" or \"inactive\"), page, per_page."
    )]
    async fn gitlab_pipeline_schedules_list(
        &self,
        Parameters(p): Parameters<PipelineSchedulesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, pipeline_schedules_list, p, "pipeline schedules")
    }

    #[tool(description = "Get a single GitLab pipeline schedule by project ID and schedule ID.")]
    async fn gitlab_pipeline_schedules_get(
        &self,
        Parameters(p): Parameters<PipelineScheduleGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipeline_schedule_get, p, "pipeline schedule")
    }

    #[tool(
        description = "List pipelines triggered by a pipeline schedule. Optional filters: status, scope, sort, created_after, created_before, updated_after, updated_before, page, per_page."
    )]
    async fn gitlab_pipeline_schedules_pipelines_list(
        &self,
        Parameters(p): Parameters<PipelineSchedulePipelinesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            pipeline_schedule_pipelines_list,
            p,
            "schedule pipelines"
        )
    }

    #[tool(
        description = "Create a new pipeline schedule. Required: project_id, cron, description, ref. Optional: active, cron_timezone."
    )]
    async fn gitlab_pipeline_schedules_create(
        &self,
        Parameters(p): Parameters<PipelineScheduleCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, pipeline_schedule_create, p, "pipeline schedule")
    }

    #[tool(
        description = "Update an existing GitLab pipeline schedule. All fields optional: cron, description, ref, active, cron_timezone."
    )]
    async fn gitlab_pipeline_schedules_update(
        &self,
        Parameters(p): Parameters<PipelineScheduleUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, pipeline_schedule_update, p, "pipeline schedule")
    }

    #[tool(description = "Delete a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_delete(
        &self,
        Parameters(p): Parameters<PipelineScheduleDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, pipeline_schedule_delete, p, "pipeline schedule")
    }

    #[tool(description = "Take ownership of a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_take_ownership(
        &self,
        Parameters(p): Parameters<PipelineScheduleTakeOwnershipParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            pipeline_schedule_take_ownership,
            p,
            "taking ownership of",
            "pipeline schedule"
        )
    }

    #[tool(description = "Run a GitLab pipeline schedule immediately (trigger now).")]
    async fn gitlab_pipeline_schedules_play(
        &self,
        Parameters(p): Parameters<PipelineSchedulePlayParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            pipeline_schedule_play,
            p,
            "playing",
            "pipeline schedule"
        )
    }

    #[tool(
        description = "Create a variable for a GitLab pipeline schedule. Required: project_id, pipeline_schedule_id, key, value. Optional: variable_type (\"env_var\" or \"file\")."
    )]
    async fn gitlab_pipeline_schedules_variables_create(
        &self,
        Parameters(p): Parameters<PipelineScheduleVariableCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            pipeline_schedule_variable_create,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Get a variable from a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_get(
        &self,
        Parameters(p): Parameters<PipelineScheduleVariableGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipeline_schedule_variable_get,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Update a variable in a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_update(
        &self,
        Parameters(p): Parameters<PipelineScheduleVariableUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            pipeline_schedule_variable_update,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Delete a variable from a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_delete(
        &self,
        Parameters(p): Parameters<PipelineScheduleVariableDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            pipeline_schedule_variable_delete,
            p,
            "pipeline schedule variable"
        )
    }
}
