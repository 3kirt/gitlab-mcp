use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{PaginationParams, QueryBuilder, encode_path_segment, encode_project_id};

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

pub async fn branches_list(
    client: &GitlabClient,
    p: BranchesListParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/branches",
        encode_project_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("regex", p.regex)
        .opt("search", p.search)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.get_with_params(&path, &params).await
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
        "/api/v4/projects/{}/repository/branches/{}",
        encode_project_id(&p.project_id),
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
    let path = format!(
        "/api/v4/projects/{}/repository/branches",
        encode_project_id(&p.project_id)
    );
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
        "/api/v4/projects/{}/repository/branches/{}",
        encode_project_id(&p.project_id),
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
    let path = format!(
        "/api/v4/projects/{}/repository/merged_branches",
        encode_project_id(&p.project_id)
    );
    client.delete(&path).await
}
