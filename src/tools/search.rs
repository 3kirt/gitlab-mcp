use serde::Deserialize;

use crate::client::{GitlabClient, ListResult};
use crate::tools::{PaginationParams, QueryBuilder, group_path, list_paginated, project_path};

// --------------------------------------------------------------------------
// Shared search filters
// --------------------------------------------------------------------------

const SCOPE_DESCRIPTION: &str = "Search scope: projects, issues, merge_requests, milestones, snippet_titles, users, wiki_blobs, commits, blobs, notes (some require Premium/Ultimate)";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchFilters {
    #[schemars(description = SCOPE_DESCRIPTION)]
    pub scope: String,
    #[schemars(description = "Search term")]
    pub search: String,
    #[schemars(description = "Search type: basic, advanced, or zoekt (default: basic)")]
    pub search_type: Option<String>,
    #[schemars(description = "Order by field (currently only supports \"created_at\")")]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: asc or desc")]
    pub sort: Option<String>,
    #[schemars(description = "Filter by confidentiality (for issues)")]
    pub confidential: Option<bool>,
    #[schemars(description = "Filter by state")]
    pub state: Option<String>,
    #[schemars(description = "Search only in specific fields (requires Premium/Ultimate)")]
    pub fields: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Build the shared search query params, returning the pagination fields
/// separately so the caller can drive [`list_paginated`].
fn search_query(f: SearchFilters) -> (QueryBuilder, PaginationParams) {
    let qb = QueryBuilder::new()
        .opt("scope", Some(f.scope))
        .opt("search", Some(f.search))
        .opt("search_type", f.search_type)
        .opt("order_by", f.order_by)
        .opt("sort", f.sort)
        .opt("confidential", f.confidential)
        .opt("state", f.state)
        .opt("fields", f.fields);
    (qb, f.pagination)
}

// --------------------------------------------------------------------------
// Global instance search
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GlobalSearchParams {
    #[serde(flatten)]
    pub filters: SearchFilters,
}

pub async fn global_search(client: &GitlabClient, p: GlobalSearchParams) -> ListResult {
    let (qb, pagination) = search_query(p.filters);
    list_paginated(client, "/api/v4/search", qb, pagination).await
}

// --------------------------------------------------------------------------
// Group search
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupSearchParams {
    pub group_id: crate::tools::GroupId,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

pub async fn group_search(client: &GitlabClient, p: GroupSearchParams) -> ListResult {
    let path = format!("{}/search", group_path(&p.group_id));
    let (qb, pagination) = search_query(p.filters);
    list_paginated(client, &path, qb, pagination).await
}

// --------------------------------------------------------------------------
// Project search
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectSearchParams {
    pub project_id: crate::tools::ProjectId,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

pub async fn project_search(client: &GitlabClient, p: ProjectSearchParams) -> ListResult {
    let path = format!("{}/search", project_path(&p.project_id));
    let (qb, pagination) = search_query(p.filters);
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

#[tool_router(router = tool_router_search, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "Search across the entire GitLab instance. Required: scope (projects, issues, merge_requests, milestones, snippet_titles, users, wiki_blobs, commits, blobs, notes), search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_global(
        &self,
        Parameters(p): Parameters<GlobalSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, global_search, p, "search results")
    }

    #[tool(
        description = "Search within a group. Required: group_id, scope, search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_group(
        &self,
        Parameters(p): Parameters<GroupSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, group_search, p, "search results")
    }

    #[tool(
        description = "Search within a project. Required: project_id, scope, search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_project(
        &self,
        Parameters(p): Parameters<ProjectSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, project_search, p, "search results")
    }
}
