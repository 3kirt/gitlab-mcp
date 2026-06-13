use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, list_paginated, project_path};

// --------------------------------------------------------------------------
// List repository tree
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoTreeListParams {
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/repository/tree", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .opt("path", p.path)
        .opt("ref", p.ref_name)
        .opt("recursive", p.recursive)
        .opt("pagination", p.pagination_mode)
        .opt("page_token", p.page_token);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get blob information
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Blob SHA to retrieve; returns content (Base64 encoded), encoding, sha, and size"
    )]
    pub sha: String,
}

pub async fn repo_blob_get(
    client: &GitlabClient,
    p: RepoBlobGetParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/repository/blobs/{}", project_path(&p.project_id), p.sha);
    client.get(&path).await
}

// --------------------------------------------------------------------------
// Get raw blob content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoBlobRawParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Blob SHA to retrieve; returns raw content (best for text files)")]
    pub sha: String,
}

pub async fn repo_blob_raw(
    client: &GitlabClient,
    p: RepoBlobRawParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/blobs/{}/raw",
        project_path(&p.project_id),
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
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/repository/compare", project_path(&p.project_id));
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
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/repository/contributors", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("ref", p.ref_name);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get merge base
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoMergeBaseParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Two or more refs (commit SHAs, branch names, or tag names) to find the common ancestor of"
    )]
    pub refs: Vec<String>,
}

pub async fn repo_merge_base(
    client: &GitlabClient,
    p: RepoMergeBaseParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/repository/merge_base", project_path(&p.project_id));
    let params: Vec<(&str, String)> = p.refs.into_iter().map(|r| ("refs[]", r)).collect();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Generate changelog data (GET — returns markdown, does not commit)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RepoChangelogGetParams {
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/repository/changelog", project_path(&p.project_id));
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
    pub project_id: crate::tools::ProjectId,
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
    let path = format!("{}/repository/changelog", project_path(&p.project_id));
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
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Generate a new health report if one does not already exist (default: false)"
    )]
    pub generate: Option<bool>,
}

pub async fn repo_health(client: &GitlabClient, p: RepoHealthParams) -> Result<Value, GitlabError> {
    let path = format!("{}/repository/health", project_path(&p.project_id));
    let params = QueryBuilder::new()
        .opt("generate", p.generate)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_repositories, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List files and directories in a GitLab repository tree. Optional: path (subdirectory), ref (branch/tag/SHA), recursive, pagination mode (keyset), page_token, page, per_page."
    )]
    async fn gitlab_repo_tree(
        &self,
        Parameters(p): Parameters<RepoTreeListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, repo_tree_list, p, "repository tree")
    }

    #[tool(
        description = "Get metadata for a GitLab repository blob (file) by its SHA. Returns content (Base64 encoded), encoding, sha, and size in bytes."
    )]
    async fn gitlab_repo_blob_get(
        &self,
        Parameters(p): Parameters<RepoBlobGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_blob_get, p, "blob")
    }

    #[tool(
        description = "Get the raw text content of a GitLab repository blob by its SHA. Best suited for text files; binary files may not decode cleanly."
    )]
    async fn gitlab_repo_blob_raw(
        &self,
        Parameters(p): Parameters<RepoBlobRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_blob_raw, p, "raw blob")
    }

    #[tool(
        description = "Compare two refs (branches, tags, or commit SHAs) in a GitLab repository. Returns commit list, diffs, and comparison metadata. Optional: from_project_id, straight (direct diff), unidiff (unified format)."
    )]
    async fn gitlab_repo_compare(
        &self,
        Parameters(p): Parameters<RepoCompareParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_compare, p, "repository comparison")
    }

    #[tool(
        description = "List contributors for a GitLab repository with commit counts, additions, and deletions. Optional: order_by (name/email/commits), sort (asc/desc), ref (branch/tag/SHA), page, per_page."
    )]
    async fn gitlab_repo_contributors(
        &self,
        Parameters(p): Parameters<RepoContributorsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, repo_contributors_list, p, "contributors")
    }

    #[tool(
        description = "Find the common ancestor (merge base) of two or more refs (commit SHAs, branch names, or tag names) in a GitLab repository."
    )]
    async fn gitlab_repo_merge_base(
        &self,
        Parameters(p): Parameters<RepoMergeBaseParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_merge_base, p, "merge base")
    }

    #[tool(
        description = "Generate changelog markdown for a semantic version without committing it. Required: project_id, version. Optional: config_file, config_file_ref, from, to, trailer, date."
    )]
    async fn gitlab_repo_changelog_get(
        &self,
        Parameters(p): Parameters<RepoChangelogGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_changelog_get, p, "changelog")
    }

    #[tool(
        description = "Generate changelog for a semantic version and commit it to the repository. Required: project_id, version. Optional: branch, config_file, config_file_ref, file, from, to, message, trailer, date."
    )]
    async fn gitlab_repo_changelog_add(
        &self,
        Parameters(p): Parameters<RepoChangelogAddParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, repo_changelog_add, p, "changelog")
    }

    #[tool(
        description = "Get repository health statistics for a GitLab project, including size, references, objects, commit graph, and bitmap information. Optional: generate (create a report if none exists)."
    )]
    async fn gitlab_repo_health(
        &self,
        Parameters(p): Parameters<RepoHealthParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repo_health, p, "repository health")
    }
}
