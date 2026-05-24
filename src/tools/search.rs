use serde::Deserialize;

use crate::client::{GitlabClient, ListResult};
use crate::tools::{PaginationParams, QueryBuilder, encode_namespace_id};

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

fn search_params(f: SearchFilters) -> Vec<(&'static str, String)> {
    QueryBuilder::new()
        .opt("scope", Some(f.scope))
        .opt("search", Some(f.search))
        .opt("search_type", f.search_type)
        .opt("order_by", f.order_by)
        .opt("sort", f.sort)
        .opt("confidential", f.confidential)
        .opt("state", f.state)
        .opt("fields", f.fields)
        .opt("page", f.pagination.page)
        .opt("per_page", f.pagination.per_page)
        .into_params()
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
    client
        .list("/api/v4/search", &search_params(p.filters))
        .await
}

// --------------------------------------------------------------------------
// Group search
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupSearchParams {
    #[schemars(description = "Group ID or URL-encoded path")]
    pub group_id: String,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

pub async fn group_search(client: &GitlabClient, p: GroupSearchParams) -> ListResult {
    let path = format!("/api/v4/groups/{}/search", encode_namespace_id(&p.group_id));
    client.list(&path, &search_params(p.filters)).await
}

// --------------------------------------------------------------------------
// Project search
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectSearchParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

pub async fn project_search(client: &GitlabClient, p: ProjectSearchParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/search",
        encode_namespace_id(&p.project_id)
    );
    client.list(&path, &search_params(p.filters)).await
}
