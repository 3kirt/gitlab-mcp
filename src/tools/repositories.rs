use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, paginate};

// --------------------------------------------------------------------------
// List repository tree
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoTreeListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[schemars(description = "Subdirectory path to list (default: repository root)")]
    pub path: Option<String>,
    #[serde(rename = "ref")]
    #[schemars(
        rename = "ref",
        description = "Branch, tag, or commit SHA to list (default: default branch)"
    )]
    pub ref_name: Option<String>,
    #[schemars(description = "Recurse into subdirectories (default: false)")]
    pub recursive: Option<bool>,
    #[serde(rename = "pagination")]
    #[schemars(
        rename = "pagination",
        description = "Pagination mode: omit for offset-based or \"keyset\" for keyset-based"
    )]
    pub pagination_mode: Option<String>,
    #[schemars(
        description = "Tree record ID to use as the first entry for keyset-based pagination"
    )]
    pub page_token: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn repo_tree_list(client: &GitlabClient, p: RepoTreeListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/repository/tree",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("path", p.path)
        .opt("ref", p.ref_name)
        .opt("recursive", p.recursive)
        .opt("pagination", p.pagination_mode)
        .opt("page_token", p.page_token)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    paginate(
        client,
        &path,
        &params,
        p.pagination.fetch_all.unwrap_or(false),
    )
    .await
}

// --------------------------------------------------------------------------
// Get blob information
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Blob SHA to retrieve; returns content (Base64 encoded), encoding, sha, and size"
    )]
    pub sha: String,
}

pub async fn repo_blob_get(
    client: &GitlabClient,
    p: RepoBlobGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/blobs/{}",
        encode_namespace_id(&p.project_id),
        p.sha
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get raw blob content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobRawParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Blob SHA to retrieve; returns raw content (best for text files)")]
    pub sha: String,
}

pub async fn repo_blob_raw(
    client: &GitlabClient,
    p: RepoBlobRawParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/blobs/{}/raw",
        encode_namespace_id(&p.project_id),
        p.sha
    );
    let content = client.get_text(&path, &[]).await?;
    Ok(json!({"content": content}))
}

// --------------------------------------------------------------------------
// Compare branches, tags, or commits
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoCompareParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Starting commit SHA or branch/tag name")]
    pub from: String,
    #[schemars(description = "Ending commit SHA or branch/tag name")]
    pub to: String,
    #[schemars(description = "Project ID to compare from (for cross-project comparison)")]
    pub from_project_id: Option<u64>,
    #[schemars(description = "Use straight diff instead of three-way diff (default: false)")]
    pub straight: Option<bool>,
    #[schemars(description = "Return diffs in unified diff format (default: false)")]
    pub unidiff: Option<bool>,
}

pub async fn repo_compare(
    client: &GitlabClient,
    p: RepoCompareParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/compare",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("from", Some(p.from))
        .opt("to", Some(p.to))
        .opt("from_project_id", p.from_project_id)
        .opt("straight", p.straight)
        .opt("unidiff", p.unidiff)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// List contributors
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoContributorsListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Order by: \"name\", \"email\", or \"commits\" (default: commits)")]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: asc)")]
    pub sort: Option<String>,
    #[serde(rename = "ref")]
    #[schemars(
        rename = "ref",
        description = "Branch, tag, or commit SHA to scope contributors to"
    )]
    pub ref_name: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn repo_contributors_list(
    client: &GitlabClient,
    p: RepoContributorsListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/repository/contributors",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("ref", p.ref_name)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    paginate(
        client,
        &path,
        &params,
        p.pagination.fetch_all.unwrap_or(false),
    )
    .await
}

// --------------------------------------------------------------------------
// Get merge base
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoMergeBaseParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Two or more refs (commit SHAs, branch names, or tag names) to find the common ancestor of"
    )]
    pub refs: Vec<String>,
}

pub async fn repo_merge_base(
    client: &GitlabClient,
    p: RepoMergeBaseParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/merge_base",
        encode_namespace_id(&p.project_id)
    );
    let params: Vec<(&str, String)> = p.refs.into_iter().map(|r| ("refs[]", r)).collect();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Generate changelog data (GET — returns markdown, does not commit)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoChangelogGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Semantic version string for the changelog (e.g. \"1.0.0\")")]
    pub version: String,
    #[schemars(
        description = "Path to changelog config file (default: .gitlab/changelog_config.yml)"
    )]
    pub config_file: Option<String>,
    #[schemars(description = "Git reference for the config file")]
    pub config_file_ref: Option<String>,
    #[schemars(description = "Starting commit SHA (excluded from the changelog)")]
    pub from: Option<String>,
    #[schemars(description = "Ending commit SHA (default: HEAD of default branch)")]
    pub to: Option<String>,
    #[schemars(
        description = "Git trailer name used to identify changelog commits (default: Changelog)"
    )]
    pub trailer: Option<String>,
    #[schemars(description = "Release date in ISO 8601 format (default: current date)")]
    pub date: Option<String>,
}

pub async fn repo_changelog_get(
    client: &GitlabClient,
    p: RepoChangelogGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/changelog",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("version", Some(p.version))
        .opt("config_file", p.config_file)
        .opt("config_file_ref", p.config_file_ref)
        .opt("from", p.from)
        .opt("to", p.to)
        .opt("trailer", p.trailer)
        .opt("date", p.date)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Add changelog data to file (POST — commits changelog to repository)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoChangelogAddParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Semantic version string for the changelog (e.g. \"1.0.0\")")]
    pub version: String,
    #[schemars(description = "Target branch to commit the changelog to (default: default branch)")]
    pub branch: Option<String>,
    #[schemars(
        description = "Path to changelog config file (default: .gitlab/changelog_config.yml)"
    )]
    pub config_file: Option<String>,
    #[schemars(description = "Git reference for the config file")]
    pub config_file_ref: Option<String>,
    #[schemars(description = "Output file path in the repository (default: CHANGELOG.md)")]
    pub file: Option<String>,
    #[schemars(description = "Starting commit SHA (excluded from the changelog)")]
    pub from: Option<String>,
    #[schemars(description = "Ending commit SHA (default: HEAD of target branch)")]
    pub to: Option<String>,
    #[schemars(description = "Commit message for the changelog commit")]
    pub message: Option<String>,
    #[schemars(
        description = "Git trailer name used to identify changelog commits (default: Changelog)"
    )]
    pub trailer: Option<String>,
    #[schemars(description = "Release date in ISO 8601 format")]
    pub date: Option<String>,
}

pub async fn repo_changelog_add(
    client: &GitlabClient,
    p: RepoChangelogAddParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/changelog",
        encode_namespace_id(&p.project_id)
    );
    let body = BodyBuilder::new()
        .req("version", &p.version)
        .opt("branch", p.branch)
        .opt("config_file", p.config_file)
        .opt("config_file_ref", p.config_file_ref)
        .opt("file", p.file)
        .opt("from", p.from)
        .opt("to", p.to)
        .opt("message", p.message)
        .opt("trailer", p.trailer)
        .opt("date", p.date)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Repository health
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoHealthParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Generate a new health report if one does not already exist (default: false)"
    )]
    pub generate: Option<bool>,
}

pub async fn repo_health(client: &GitlabClient, p: RepoHealthParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/repository/health",
        encode_namespace_id(&p.project_id)
    );
    let params = QueryBuilder::new()
        .opt("generate", p.generate)
        .into_params();
    client.get_with_params(&path, &params).await
}
